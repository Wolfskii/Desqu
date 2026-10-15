# Changelog

All notable changes to Workspaces. The release workflow publishes the section that matches the
version being released as the release notes and as the in-app update description.

## Unreleased

- Hiding icons on other virtual desktops works again on Windows 11 build 26300 (it no longer lets
  icons be moved off-screen): items owned by another desktop get the hidden attribute (the desktop
  reacts in under a second) and lose it when you come back, quit, or after a crash at the next start.
  System items such as the Recycle Bin cannot be hidden and are reported in Settings.

## 0.1.0

- First release: tie desktop shortcuts, files and folders to a virtual desktop so they show only
  there, new files join the desktop you are on, icons always restored (also after a crash), in-app
  updates.
