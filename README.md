# VibeDiary

A simple, private diary / journal for Windows, written in Rust with [eframe/egui](https://github.com/emilk/egui).

Write down your thoughts each day with rich formatting and pictures, tag them with colour-coded categories, rate your mood, and see at a glance which days have entries. Everything is encrypted with a password.

## Features

**Calendar**
- **Day, Week, Month, Year and Photos views.** In Day, Week and Month each entry is a coloured pill (time + first line); the Day view is an hour-by-hour timeline. A small face on a day shows its average mood.
- **Photos view**: every picture in the diary as thumbnails, newest first and grouped by month. Click one to see it large (arrow keys move between pictures) or jump to its entry.
- **Year view**: a heat-map of the days you wrote (shade = words written), plus entries, days written, words, current and longest streak, average mood, entries per category and a chart of your mood through the year.
- **Drag and drop**: drag an entry's pill onto another day to move it (its time is kept). In the Day view, drag it to another hour to change its time. Press Esc to cancel a drag; double-click a pill to edit it.
- **On this day**: the day panel lists what you wrote on the same date in earlier years; click one to jump to it.
- **Random**: the Random button (or `Ctrl+R`) jumps to a random entry from the past. Private entries are never picked.
- **Pinned entries**: click the star on an entry to pin it. Pinned entries are starred on the calendar and listed under **Pinned** in the header.
- **Insights** (header button): *Mood patterns* (which days, times of day, entry lengths and categories go with happier or lower ratings), *Writing* (most-used words, when you write, longest entries) and *Review* (a recap of a week, month or year with highlights and pictures, which can be saved as a PDF).
- **Search** across all entries from the box in the header; click a result to jump to its day.

**Writing**
- **WYSIWYG rich text editor** (Markdown underneath): type in a single page where bold looks bold, headings are large and the Markdown marks disappear. The marks show on the line your cursor is on, or everywhere with "Show formatting marks". A toolbar offers bold, italic, strikethrough, code, three heading sizes, bulleted and numbered lists and quotes.
- **Pictures** appear in the page where you put them. Add them with the **Image** button or by dropping a file onto the editor, and remove them with the button under each one. Large images are shrunk (longest side 1600 px) and stored inside the encrypted diary.
- **Mood**: rate each entry with five emoji-style faces, from really sad (red, crying) to really happy (green, grinning). Click a face again to clear it. The scale is stored as 1 to 5.
- **Focus mode**: the Focus button (or `F11`) turns the editor into a distraction-free full-window page with larger text. `Esc` leaves it.
- **Attachments**: attach any file (up to 25 MB each) with **Attach file…** or by dropping it on the editor. Files are stored inside the encrypted diary; use **Save as…** or **Open** on the entry's card to get them out. (**Open** writes a temporary, unencrypted copy that is deleted when the diary locks or the app next starts.)
- **Private entries**: tick **Private** and the entry stays hidden (its text, mood, pictures and search matches) until you click it open. It is hidden again when the diary locks or the window loses focus.
- **Word count and daily goal**: the editor shows a live word count. Pick a daily goal under **Settings → Daily word goal** and a progress bar tracks the day's total in the editor and the day panel.
- **Multiple entries per day**, each with a time and a category. You can change an entry's date and time when you edit it.
- **Categories** with their own colours. Add, rename, recolour and delete them from the Categories window. Renaming a category updates the entries that use it.

**Privacy and data**
- **Password protection.** The diary is encrypted with a password you choose, and the app asks for it every time it starts. **Lock** locks it immediately.
- **Recovery key** (**Settings → Recovery key**): a long code, shown once, that can open the diary if you forget your password. On the unlock screen choose **Forgot your password?**, enter the key and pick a new password. You can make a new key (the old one then stops working) or remove it. You are offered one when you first create a diary.
- **Recently deleted** (**Settings → Recently deleted**): deleting an entry moves it to a bin for 30 days, with an **Undo** straight away, **Restore**, **Delete for good** and **Empty the bin**. Pictures and attachments are kept while an entry is in the bin.
- **Auto-lock** after a period of inactivity (default 10 minutes; change or turn off under **Settings → Auto-lock**). An entry you're in the middle of writing survives locking.
- **Privacy screen**: the diary is hidden while its window is in the background or minimised (on by default; switch it off under **Settings**).
- **Text size**: **Settings → Text size**, or `Ctrl +` / `Ctrl -` (and `Ctrl 0` to reset), makes everything bigger or smaller; the choice is remembered.
- **Export to PDF** (**Settings → Export to PDF**, or `Ctrl+E`): a day, week, month, year or everything, with or without pictures. Private entries are left out unless you tick the box for them.
- **Backup**: **Settings → Back up encrypted copy** saves a copy of the encrypted file wherever you like.
- **Data location**: **Settings → Data location** shows where the diary lives, moves it to another folder, or opens a diary (such as a backup) from another folder.

**Look and feel**
- Popup windows are draggable and resizable; light and dark mode follow your Windows setting.
- Keyboard shortcuts (see below, or **Settings → Keyboard shortcuts**).

## Using the app

| Action | How |
| --- | --- |
| Select a day | Click it on the calendar |
| New entry | Double-click a day, press `Ctrl+N`, or use **New entry** / **Add entry for this day** |
| Edit or delete an entry | Use the buttons on its card in the day panel, or double-click its pill |
| Format text | Select text and use the toolbar buttons (click again to remove the format), or type Markdown such as `**bold**` or `# Heading` |
| Add a picture | **Image…** in the editor, or drop an image file onto the editor |
| Switch view | **Day / Week / Month / Year / Photos** buttons above the calendar, or `Ctrl+1` … `Ctrl+5` |
| Look back | **Random** or `Ctrl+R` |
| Focus mode | **Focus** in the editor, or `F11` (`Esc` leaves it) |
| Move around | **◀** / **▶** or the left/right arrow keys step a day, week, month or year; **Today** (`Ctrl+T`) jumps back |
| Move an entry | Drag its pill onto another day (or, in the Day view, another hour) |
| Search | `Ctrl+F`, then type; clear the box to go back to the day view |
| Save an entry | **Save**, `Ctrl+S` or `Ctrl+Enter` |
| Lock | **Lock** or `Ctrl+L` |

Other editor shortcuts: `Ctrl+B` bold, `Ctrl+I` italic.

Deleting a category does not delete its entries; they are shown in grey.

## Running it

You need the [Rust toolchain](https://rustup.rs/). From the project folder:

```
cargo run
```

Use `cargo run --release` for an optimised build. The build embeds the app icon in the `.exe`, which needs the Windows SDK's resource compiler (installed with the Visual Studio C++ build tools). If it is missing, the build still succeeds with a warning and the file just keeps the default icon.

## Where your data is stored

Entries, categories, images and settings are saved, encrypted, to `entries.json` in `%APPDATA%\VibeDiary` by default. You can choose another folder from **Settings → Data location**; the choice is remembered in `%APPDATA%\VibeDiary\config.json`.

Earlier development builds kept the diary next to the source code. If a diary is found there and none exists yet in the new location, it is **copied** across the first time you run the app (the original is left alone, so you can delete it once you are happy).

## Encryption

- The diary is sealed with **XChaCha20-Poly1305** under a random 256-bit data key, which also detects a damaged or tampered file. That data key is stored only in "wrapped" (encrypted) form: once under a key stretched from your password with **Argon2id** (64 MiB, 3 passes), and, if you made one, once under a key stretched from your recovery key. Either opens the diary. Changing the password re-wraps the data key, so the recovery key keeps working.
- `entries.json` is a small JSON envelope holding the key-derivation settings, the wrapped keys, a nonce and the ciphertext. Nothing in it reveals your entries or pictures, and the recovery key itself is never stored. Diaries from before recovery keys are upgraded to this format automatically the first time you unlock them (after that, older versions of the app can't open the file).
- Every save uses a fresh nonce, is checked by decrypting it again, and replaces the old file atomically.
- **Without a recovery key there is no way back.** If you forget the password and have no recovery key, nobody can open the diary. That is what makes the encryption real. Keep a recovery key, or a backup whose password you remember.
- Passwords must be at least 8 characters.
- An unencrypted `entries.json` from an earlier version is offered for encryption the first time you run the app. It is replaced by the encrypted version and no plain-text backup is kept.
- Text and pictures held in memory while the app is unlocked are not protected. Use **Lock** if you step away.
- Because pictures are inside the file, a diary with many large pictures makes every save slower and the file bigger.
- **PDF exports are not encrypted.** They are a plain copy of what you chose to export.

Once decrypted, the data looks like this:

```json
{
  "categories": [
    { "name": "Personal", "color": [96, 165, 250] }
  ],
  "entries": [
    {
      "id": 1,
      "date": "2026-10-08",
      "added_at": "2026-10-08 16:11:27",
      "category": "Personal",
      "text": "# A good day\n\nSome **bold** thoughts.\n\n![photo](img:i1760000000000)",
      "mood": 4,
      "pinned": false,
      "sensitive": false,
      "attachments": [{ "id": "f1760000000001", "name": "ticket.pdf", "size": 48213 }]
    }
  ],
  "images": { "i1760000000000": "<base64 JPEG or PNG>" },
  "files": { "f1760000000001": "<base64 file>" },
  "settings": { "auto_lock_minutes": 10, "daily_word_goal": 250, "ui_scale": 1.0, "privacy_screen": true }
}
```

## PDF export notes

- Fonts come from Windows (Segoe UI, plus Consolas when an entry contains code) and are embedded in the PDF, so a PDF is a few megabytes even for a single day.
- Attachments are listed by name but not included in the PDF.
- Characters the fonts don't have, such as emoji, appear as empty boxes.
- Strikethrough text is shown in grey rather than struck through.
- Pictures are shrunk to 900 px and transparency is flattened onto white.

## Project layout

- `src/main.rs` contains the application: the UI, lock screen, calendar views, editor and file handling.
- `src/markdown.rs` parses and renders entries for display, counts words, and holds the toolbar's text-formatting logic.
- `src/live.rs` styles the text as you type (the WYSIWYG layout) and splits an entry into text and picture blocks.
- `src/stats.rs` works out totals, streaks, the heat-map levels, the mood series and "On this day".
- `src/reports.rs` works out mood patterns, writing statistics and week / month / year reviews; `src/insights.rs` is the Insights window that shows them.
- `src/gallery.rs` is the Photos view and picture viewer; `src/pinned.rs` is the Pinned list; `src/faces.rs` draws the mood faces.
- `src/export.rs` builds the PDF.
- `src/location.rs` decides where the diary file lives and moves it.
- `src/vault.rs` contains the encryption: the password, the recovery key and the wrapped data key. `src/recovery.rs` is the Recovery key window; `src/trash.rs` is Recently deleted.
- `src/icon.rs` draws the app icon; `build.rs` turns it into the `.exe` icon.
- `Cargo.toml` lists the dependencies: `eframe`/`egui_extras` (UI), `chrono`/`jiff` (dates), `serde`/`serde_json`, `pulldown-cmark` (Markdown), `image` (pictures), `rfd` (file dialogs), `argon2`/`chacha20poly1305`/`base64`/`getrandom` (encryption), `genpdf`/`lopdf` (PDF), and `winresource` (build only).
