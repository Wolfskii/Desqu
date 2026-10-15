# Changelog

All notable changes to Vials. The release workflow publishes the section that matches the version
being released as the release notes and as the in-app update description.

## Unreleased

- Right-click an icon in a vial: Explorer's own context menu for that file, folder or shortcut
  (Shift for the extended one) instead of the web view's browser menu.
- Windows 11 build 26300 (October 2026 updates) pulls icons that were moved off-screen back onto the
  desktop, so items in a vial stayed visible next to it. Items in a vial are now hidden by giving the
  file the hidden attribute instead (nothing else about it changes; Windows' "Show hidden files" must
  be off): the desktop stays Windows' own, with its own menus, drag and drop and icon placement. The
  mark is removed when an item leaves its vial, when Vials stops, and after a crash at the next start.
- New setting "Draw the whole desktop in Vials" (off by default): Windows' own desktop icons are
  switched off, as "View, Show desktop icons" does, and a built-in "Desktop" vial shows every item in
  no other vial. For people who have hidden files shown.

## 0.1.0

- First release: Fences-style desktop vials. Drag a box on the empty desktop to create one, drag
  icons into it, auto-sort rules, items hidden from the loose desktop and always restored (also
  after a crash), in-app updates.
