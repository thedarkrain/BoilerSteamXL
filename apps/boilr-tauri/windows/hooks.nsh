; BoilerSteamXL installer hooks.
; On uninstall: remove the app's own scheduled task. App data (settings, review queue,
; backups) is only deleted when the user ticks "Delete the application data".

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'schtasks.exe /Delete /TN "BoilerSteamXL Library Refresh" /F'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
    RMDir /r "$APPDATA\BoilerSteamXL"
  ${EndIf}
!macroend
