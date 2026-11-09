// A sentence turned into a search filter on this PC, without asking any AI: used when the
// local index is ready. A few rules read the kind of file, sizes and dates ("videos over
// 500 MB I haven't opened in a year"); what is left ("of the holidays") is the topic the
// index looks for. Italian and English.
import type { SearchFilter, SearchKind, SearchPlace } from "./types";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

const KIND_WORDS: [SearchKind, RegExp][] = [
  ["video", /\b(video|videos|filmati?|film|movies?|clips?)\b/gi],
  ["image", /\b(foto|fotografie|immagini|immagine|photos?|pictures?|images?)\b/gi],
  ["document", /\b(documenti|documento|documents?|pdf)\b/gi],
  ["audio", /\b(musica|canzoni|canzone|audio|music|songs?|podcasts?)\b/gi],
];

const UNIT: Record<string, number> = { kb: KB, mb: MB, gb: GB, tb: GB * 1024 };
const NUMBER = String.raw`(\d+(?:[.,]\d+)?)\s*(kb|mb|gb|tb)\b`;
const BIGGER = new RegExp(String.raw`\b(?:più grandi di|più grande di|oltre|sopra|almeno|over|larger than|bigger than|more than|at least|above)\s+${NUMBER}`, "gi");
const SMALLER = new RegExp(String.raw`\b(?:più piccoli di|più piccolo di|sotto|meno di|under|smaller than|less than|below)\s+${NUMBER}`, "gi");
const SIZE_ALONE = new RegExp(String.raw`\b${NUMBER}`, "gi");

/** "un anno", "2 mesi", "a year", "6 months" in days. */
const SPAN = String.raw`(un|una|uno|a|an|one|\d+)\s+(anni|anno|mesi|mese|settimane|settimana|giorni|giorno|years?|months?|weeks?|days?)`;
const UNUSED = new RegExp(
  String.raw`\b(?:che\s+)?(?:non\s+(?:apro|aperti|aperte|uso|usati|usate|guardo|guardati|ascolto|tocco)|(?:i\s+)?(?:haven'?t|have not|not|never)\s+(?:opened|used|watched|touched|played)|unused|untouched)\s+(?:da|in|for|da più di|da almeno)\s+(?:(?:più di|almeno|over|more than)\s+)?${SPAN}`,
  "gi",
);
const RECENT = new RegExp(String.raw`\b(?:(?:degli|negli|nelle|of the|in the|from the)\s+)?(?:ultim[ie]|ultima|ultimo|last|past)\s+${SPAN}`, "gi");
const RECENT_WORDS: [RegExp, number][] = [
  [/\b(oggi|today)\b/gi, 1],
  [/\b(questa settimana|this week)\b/gi, 7],
  [/\b(questo mese|this month)\b/gi, 30],
  [/\b(quest'anno|questo anno|this year)\b/gi, 365],
  [/\b(l'anno scorso|dell'anno scorso|last year)\b/gi, 730],
];
const PLACES: [SearchPlace, RegExp][] = [
  ["downloads", /\b(?:nei|nella cartella|in|from|nella|tra i)?\s*download\b/gi],
  ["desktop", /\b(?:sul|sulla|on the|on my|from the)?\s*(?:scrivania|desktop)\b/gi],
];

/** Words that say nothing about what the files show. */
const FILLER = new Set(
  "che di del della delle dei degli dello da dal dalla dai dagli in nel nella nei negli nelle con su sul sulla sui per tra fra il lo la i gli le un una uno e o mio mia miei mie tuo tua tutti tutte tutto i miei le mie cerca trova trovami mostra mostrami voglio vorrei file files the a an of my all find show me i want with in on at from for and or that which some any".split(
    " ",
  ),
);

function days(amount: string, unit: string): number {
  const n = /^\d+$/.test(amount) ? Number(amount) : 1;
  const u = unit.toLowerCase();
  const per = /^(anni|anno|year)/.test(u) ? 365 : /^(mesi|mese|month)/.test(u) ? 30 : /^(settiman|week)/.test(u) ? 7 : 1;
  return Math.min(36_500, n * per);
}

const bytes = (n: string, unit: string) => Math.round(Number(n.replace(",", ".")) * UNIT[unit.toLowerCase()]);

/** The filter a sentence asks for; `topic` is what is left once kinds, sizes and dates are read. */
export function readSentence(sentence: string): SearchFilter {
  let rest = ` ${sentence.trim()} `;
  const take = (re: RegExp, on: (m: RegExpExecArray) => void) => {
    rest = rest.replace(re, (...args) => {
      on(args.slice(0, -2) as unknown as RegExpExecArray);
      return " ";
    });
  };

  let minBytes: number | null = null;
  let maxBytes: number | null = null;
  let unusedDays: number | null = null;
  let recentDays: number | null = null;
  take(BIGGER, (m) => (minBytes = bytes(m[1], m[2])));
  take(SMALLER, (m) => (maxBytes = bytes(m[1], m[2])));
  if (minBytes === null && maxBytes === null) take(SIZE_ALONE, (m) => (minBytes = bytes(m[1], m[2])));
  take(UNUSED, (m) => (unusedDays = days(m[1], m[2])));
  take(RECENT, (m) => (recentDays = days(m[1], m[2])));
  for (const [re, d] of RECENT_WORDS) take(re, () => (recentDays = d));

  const kinds: SearchKind[] = [];
  for (const [kind, re] of KIND_WORDS) take(re, () => !kinds.includes(kind) && kinds.push(kind));
  const places: SearchPlace[] = [];
  for (const [place, re] of PLACES) take(re, () => !places.includes(place) && places.push(place));

  const words = rest
    .replace(/[^\p{L}\p{N}'’ -]+/gu, " ")
    .split(/\s+/)
    .filter((w) => w && !FILLER.has(w.toLowerCase()));
  const topic = words.join(" ").trim();
  return { kinds, minBytes, maxBytes, unusedDays, recentDays, places, words: [], sort: "size", topic: topic || null, loose: false };
}

/**
 * Words that say a size, a date or an order the rules above did not read: left in the
 * topic, they would be looked for as what the files show. Then the AI is asked, if it is on.
 */
const UNREAD = /\d|\b(più|meno|oltre|sotto|prima|dopo|fino|mesi|mese|anni|anno|settimane|giorni|ieri|scors[oa]|grandi|piccoli|pesanti|vecchi|recenti|nuovi|ultimi|more|less|over|under|before|after|since|until|between|months?|years?|weeks?|days?|yesterday|last|big|bigger|large|larger|small|smaller|heavy|old|older|recent|newest|oldest)\b/i;

/** The sentence holds something the rules could not read (an edge case for the AI). */
export const needsHelp = (filter: SearchFilter): boolean => !!filter.topic && UNREAD.test(filter.topic);

