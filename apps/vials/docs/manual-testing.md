# Manual test procedures

The automated smoke test (`pwsh apps/vials/scripts/smoke-test.ps1`) covers parking, restore, the
vial window styles, `--quit` and crash recovery with one temporary file. The cases below need a
human and a real desktop. Record result + date next to each case.

Use a spare file or shortcut on the desktop, not something you cannot afford to lose track of:
Vials never moves or deletes files, but items are hidden while a vial holds them.

## Fences-style creation and drag-in

0a. With Vials on, drag a box over empty desktop space (at least ~90x70 px). Release. Expect a small
    "Create vial here" menu at the cursor within about a third of a second. Record the delay.
0b. Click it. Expect a new translucent vial exactly where you dragged, named "Vial 1" (next free
    number). Rename it in the Vials window.
0c. Click elsewhere or press Esc while the menu is open. Expect it to vanish and nothing created.
0d. Drag a box that catches at least one icon. Expect the normal selection and no menu.
0e. Start a drag on an icon (moving it). Expect no menu.
0f. Single click on empty desktop. Expect no menu.
0g. Drag an icon from the desktop onto the vial. Expect the vial to highlight while you hover and
    the icon to move into the vial (and disappear from the loose desktop) a moment after you drop.
0h. Drag a file from an Explorer window (not the desktop) onto the vial. Expect nothing to happen.
0g2. Drag an icon out of the vial onto the empty desktop. Expect the normal Windows drag (icon follows
    the cursor), the icon to land where you dropped it and to be gone from the vial.
0g3. Drag an icon from one vial onto another vial. Expect it to move there.
0g4. Drag an icon out of a vial into an open Explorer folder window. Expect Windows to move the real
    file (Ctrl copies, Alt makes a shortcut, Esc cancels with nothing changed).
0g4b. Click a vial's name on the desktop. Expect an edit box with the name selected; type, Enter
    saves, Esc cancels, clicking elsewhere saves. Afterwards the vial must be back under other windows
    and never take focus again.
0g4c. Drag a box and wait: the popup must appear fully drawn (no blank flash) and stay until you click
    elsewhere, press Esc, or choose it. Click inside it: it must not close before the click lands.
0g5. A vial has no x buttons, no item counter and no "Empty" text.
0g6. Start Vials with no config: expect no vial at all. Right-click empty desktop, "Show more
    options", "Create vial here": expect a vial with its corner at the right-click spot.
0g7. Quit Vials; right-click the desktop. Expect the "Create vial here" entry to be gone.
0i. Settings: turn off "Create vials by dragging a box". Expect no menu any more.
0j. Menu near a screen edge and on a second monitor / at 125-150% scaling: expect it fully visible.
0l. Settings: switch "Look of vials" between Glass and Classic. Expect the vials to be rebuilt in the
    chosen look at the same place with the same items, and the create popup to follow the choice.
0m. Glass: try Frosted and Clear at tint 0, 20, 50 over a bright and a dark wallpaper. Pick the
    combination that looks closest to Apple's glass and tell the maintainers (the defaults are a guess:
    Frosted, 20). Check that acrylic does not make dragging a vial laggy.
0k. Check the vial looks right (translucent, rounded, readable over your wallpaper, also with a
    live wallpaper).

## Basics

1. Start Vials, **Vials** tab → add a vial. Expect a window on the desktop, below other windows.
2. Drag an item from the **Items** tab into the vial. Expect it gone from the loose desktop and
   shown in the vial with its icon.
3. Double click it in the vial. Expect it to open.
4. Press × on it. Expect it back on the desktop where it was.
5. Drag the vial by its title bar; restart Vials. Expect the same place and size.
6. Collapse the vial. Expect only the title bar to remain; expand restores it.
7. Alt+Tab and the taskbar must not list the vial windows; clicking one must not steal focus.

## Safety

8. With an item hidden, end Vials from Task Manager. Expect the item to stay hidden. Start Vials
   again (item removed from every vial first). Expect it back on the desktop.
9. Quit Vials from the tray. Expect all hidden items back, exactly where they were.
10. Restart Explorer (Task Manager → Windows Explorer → Restart) while a vial holds items.
    Record what happens to the hidden items and the vial windows.
11. Win+D (show desktop). Record whether the vials stay visible.
12. Change display scaling (125% / 150%) and move a vial to a second monitor if you have one.
    Expect the vial to keep a sensible size and position.

## Rules

13. Add a rule "extension .txt → vial X", turn auto-sort on, create a new .txt on the desktop.
    Expect it in vial X within a few seconds (it may show on the desktop for ~4 s first).
14. Take that file out of the vial by hand, create another .txt. Expect only the new one sorted.

## Inside Desqu

15. Turn Vials on in Desqu with the standalone app running. Expect an offer to take over; accept.
    Expect vials and settings unchanged and the standalone autostart entry disabled.
16. Turn Vials off in Desqu. Expect vial windows closed and every hidden item restored.
