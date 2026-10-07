; Register the Explorer right-click entries after install and remove them
; before uninstall. Everything lives under HKCU, so no elevation is needed.

!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --register'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --unregister'
!macroend
