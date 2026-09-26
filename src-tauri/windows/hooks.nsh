; Hooks of the NSIS installer (see tauri.conf.json > bundle.windows.nsis).

; Uninstalling for good (not an update) removes the pre-launch sync from the
; Lunar profiles, which otherwise would keep calling a missing launcher.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --forget'
  ${EndIf}
!macroend
