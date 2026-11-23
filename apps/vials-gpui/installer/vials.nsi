; Per-user installer for Vials (GPUI). No administrator rights needed.
;   makensis /DVERSION=0.4.0 /DEXE=..\target\release\vials-gpui.exe /DOUT=..\dist\Vials-GPUI_0.4.0_x64-setup.exe vials.nsi
; Silent install:   Vials-GPUI_x_x64-setup.exe /S            (optionally  /D=C:\some\folder  as the last argument)
; Silent uninstall: "<install folder>\uninstall.exe" /S
; The uninstaller never deletes the user's config (%APPDATA%\io.github.wolfskii.vials).

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef EXE
  !error "pass /DEXE=<path to vials-gpui.exe>"
!endif
!ifndef OUT
  !define OUT "Vials-GPUI-setup.exe"
!endif

!define APP "Vials"
!define UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\VialsGpui"

Name "${APP} ${VERSION}"
OutFile "${OUT}"
Unicode true
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\Vials GPUI"
InstallDirRegKey HKCU "Software\VialsGpui" "InstallDir"
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
  IfFileExists "$INSTDIR\vials-gpui.exe" 0 +3
    nsExec::Exec '"$INSTDIR\vials-gpui.exe" --live --action=quit'
    Sleep 1500
!macroend

Section "Install"
  SetOutPath "$INSTDIR"
  !insertmacro QuitRunningCopy
  File "${EXE}"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateShortcut "$SMPROGRAMS\Vials.lnk" "$INSTDIR\vials-gpui.exe" "--live" "$INSTDIR\vials-gpui.exe" 0

  WriteRegStr HKCU "Software\VialsGpui" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayName" "${APP} (GPUI)"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "Publisher" "Desqu contributors"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\vials-gpui.exe"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL_KEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTALL_KEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  !insertmacro QuitRunningCopy
  Delete "$INSTDIR\vials-gpui.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\Vials.lnk"
  ; What the app itself may have added: its start-with-Windows entry and the desktop's right-click entry.
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "VialsGpui"
  DeleteRegKey HKCU "Software\Classes\DesktopBackground\Shell\DesquVials"
  DeleteRegKey HKCU "Software\VialsGpui"
  DeleteRegKey HKCU "${UNINSTALL_KEY}"
SectionEnd
