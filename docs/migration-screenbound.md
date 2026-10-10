# Upgrading from ScreenBound 0.1.0

Screenbound was renamed and now stores its settings under a new name. Your settings move over
automatically the first time you start the new version. Nothing is deleted.

## Steps

1. **Quit the old version.** Right-click the ScreenBound tray icon and choose **Exit**. Closing the
   window only hides it to the tray, and two copies would fight over the same windows. If you forget,
   the new version shows "An older Screenbound is running" and stays off until the old one is gone.
2. **Install the new version** (`Screenbound_*_x64-setup.exe`). It installs over the old one, in the same
   folder, so your shortcuts and uninstall entry keep working. The portable `.exe` also works.
3. **Start it.** Your zones, rules, groups and the list of seen apps are copied across.

## What is migrated

| Before | After |
| --- | --- |
| `%APPDATA%\com.screenbound.app\config.json` (zones, rules, groups, apps) | `%APPDATA%\io.github.wolfskii.screenbound\config.json` |
| **Start at boot** setting | **Start with system** in Settings (kept as you had it) |
| Windows left constrained by a crash | Restored on first start |

The old folder `%APPDATA%\com.screenbound.app` is **left untouched** as a backup. It gets a
`MIGRATED.txt` file with the date. Settings are copied only once; running the new version again never
overwrites what you have changed since.

## Clean up (optional)

Once you have checked that everything works, open **Settings → Older Screenbound**:

- **Delete old settings…** removes `%APPDATA%\com.screenbound.app` after you confirm. You can also
  delete that folder yourself.
- **Uninstall** appears only if a *different* Screenbound installation is found (for example an old
  copy in another folder).

## If something goes wrong

Your old settings are still in `%APPDATA%\com.screenbound.app\config.json`. Copy that file over
`%APPDATA%\io.github.wolfskii.screenbound\config.json` while Screenbound is not running, then start it
again.
