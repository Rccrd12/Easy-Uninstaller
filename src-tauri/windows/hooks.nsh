; Extra steps for the NSIS installer (bundle > windows > nsis > installerHooks).

; Uninstalling removes the reminders' scheduled task (src-tauri/src/reminders.rs).
; Updates run the uninstaller too, with /UPDATE: the task stays for the new version.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$SYSDIR\schtasks.exe" /Delete /TN "Easy Uninstaller reminders" /F'
    Pop $0
  ${EndIf}
!macroend
