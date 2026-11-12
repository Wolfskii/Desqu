; Removes what the browser bridge left in this user's profile when the app is uninstalled:
; the native host registration (HKCU) and the manifest folder. The browsers' own settings are not touched.
!macro NSIS_HOOK_POSTUNINSTALL
  SetShellVarContext current
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Chromium\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\BraveSoftware\Brave-Browser\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Vivaldi\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Opera Software\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Mozilla\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\LibreWolf\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  DeleteRegKey HKCU "Software\Waterfox\NativeMessagingHosts\io.github.wolfskii.desqu.browser"
  RMDir /r "$LOCALAPPDATA\Desqu\browser-host"
!macroend
