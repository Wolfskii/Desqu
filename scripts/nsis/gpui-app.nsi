; Per-user installer for a Desqu GPUI app. No administrator rights needed.
;   makensis /DAPP=Vials /DKEY=VialsGpui /DEXEFILE=vials-gpui.exe /DEXTRA_ARGS=--live /DVERSION=0.4.0
;            /DEXE=<path to the exe> /DOUT=<setup exe> gpui-app.nsi       (scripts/package-gpui-app.ps1 does this)
; Silent install:   <setup>.exe /S            (optionally  /D=C:\some\folder  as the last argument)
; Silent uninstall: "<install folder>\uninstall.exe" /S
; The uninstaller never deletes the user's config. With /DDESKTOP_MENU_KEY=<registry key> it also removes
; that desktop right-click entry.

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef APP
  !error "pass /DAPP=<display name>"
!endif
!ifndef KEY
  !error "pass /DKEY=<letters and digits: names the registry entries>"
!endif
!ifndef EXEFILE
  !error "pass /DEXEFILE=<name of the exe once installed>"
!endif
!ifndef EXE
  !error "pass /DEXE=<path to the exe to pack>"
!endif
!ifndef OUT
  !define OUT "${APP}-GPUI-setup.exe"
!endif
!ifndef EXTRA_ARGS
  !define EXTRA_ARGS ""
!endif

!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${KEY}"

Name "${APP} ${VERSION}"
OutFile "${OUT}"
Unicode true
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\${APP} GPUI"
InstallDirRegKey HKCU "Software\${KEY}" "InstallDir"
SetCompressor /SOLID lzma
ShowInstDetails nevershow
ShowUninstDetails nevershow

VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "FileDescription" "${APP} setup"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Desqu contributors"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

; Asks a running copy to quit (a second launch hands the action to the first one) and gives it a moment.
!macro QuitRunningCopy
  IfFileExists "$INSTDIR\${EXEFILE}" 0 +3
    nsExec::Exec '"$INSTDIR\${EXEFILE}" ${EXTRA_ARGS} --action=quit'
    Sleep 1500
!macroend

Section "Install"
  SetOutPath "$INSTDIR"
  !insertmacro QuitRunningCopy
  File "${EXE}"
  ; Programs that travel with it (the Desqu shell carries its modules): /DEXE2=<path> /DEXEFILE2=<name>, up to 4.
  !ifdef EXE2
    File "${EXE2}"
  !endif
  !ifdef EXE3
    File "${EXE3}"
  !endif
  !ifdef EXE4
    File "${EXE4}"
  !endif
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Its own shortcut name, so installing or removing this never touches the Tauri build's shortcut.
  CreateShortcut "$SMPROGRAMS\${APP} (GPUI).lnk" "$INSTDIR\${EXEFILE}" "${EXTRA_ARGS}" "$INSTDIR\${EXEFILE}" 0

  WriteRegStr HKCU "Software\${KEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP} (GPUI)"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Desqu contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\${EXEFILE}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  !insertmacro QuitRunningCopy
  Delete "$INSTDIR\${EXEFILE}"
  !ifdef EXEFILE2
    Delete "$INSTDIR\${EXEFILE2}"
  !endif
  !ifdef EXEFILE3
    Delete "$INSTDIR\${EXEFILE3}"
  !endif
  !ifdef EXEFILE4
    Delete "$INSTDIR\${EXEFILE4}"
  !endif
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APP} (GPUI).lnk"
  ; What the app itself may have added: its start-with-Windows entry and, for Vials, the desktop's right-click entry.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${KEY}"
  !ifdef DESKTOP_MENU_KEY
    DeleteRegKey HKCU "${DESKTOP_MENU_KEY}"
  !endif
  DeleteRegKey HKCU "Software\${KEY}"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
