// The reminders (src-tauri/src/reminders.rs): a Windows notification when the system
// drive is almost full, and one after a month without opening the app. Reactive.
import { getReminders, previewReminder, remindersOpened, setReminders } from "./api";
import { locale, t } from "./i18n/index.svelte";
import type { ReminderSettings, ReminderTexts } from "./types";

/** The choices, loaded from Rust by `loadReminders` (defaults until then). */
export const reminders = $state<ReminderSettings>({ lowSpace: false, threshold: 10, monthly: false });

/** The notifications in the chosen language. Windows shows them when the app is closed. */
export function reminderTexts(): ReminderTexts {
  return {
    lowSpaceTitle: t("reminders.notice.lowSpaceTitle"),
    lowSpaceBody: t("reminders.notice.lowSpaceBody"),
    monthlyTitle: t("reminders.notice.monthlyTitle"),
    monthlyBody: t("reminders.notice.monthlyBody"),
    decimal: (1.5).toLocaleString(locale()).charAt(1),
  };
}

export async function loadReminders(): Promise<void> {
  Object.assign(reminders, await getReminders());
}

/** Saves new choices; on failure nothing changes and the error is thrown. */
export async function saveReminders(next: ReminderSettings): Promise<void> {
  await setReminders(next, reminderTexts());
  Object.assign(reminders, next);
}

/** Called when the app opens and when the language changes. Failures stay quiet. */
export function noteAppOpened(): void {
  remindersOpened(reminderTexts()).catch(() => {});
}

/** A sample notification, shown right away. */
export function showSampleNotice(): Promise<void> {
  return previewReminder(t("reminders.notice.testTitle"), t("reminders.notice.testBody"));
}
