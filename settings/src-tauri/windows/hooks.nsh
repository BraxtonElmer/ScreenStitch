; Extra installer steps for ScreenStitch.

!macro NSIS_HOOK_PREINSTALL
  ; The tray app keeps running in the background; close it so it can be replaced.
  nsExec::Exec 'taskkill /F /IM ScreenStitch.exe'
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Start the tray app right away (also after an automatic update).
  Exec '"$INSTDIR\ScreenStitch.exe" --background'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'taskkill /F /IM ScreenStitch.exe'
  nsExec::Exec 'taskkill /F /IM screenstitch-settings.exe'
  ; Remove "Start with Windows".
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "ScreenStitch"
!macroend
