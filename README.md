# VibeDiary

A simple, private diary / journal for Windows, written in Rust with [eframe/egui](https://github.com/emilk/egui).

Write down your thoughts each day with rich formatting and pictures, tag them with colour-coded categories, and see at a glance which days have entries on a month calendar. Everything is encrypted with a password.

## Features

- **Month, Week and Day calendar views.** Each entry is a coloured pill (time + first line). The Day view is an hour-by-hour timeline.
- **Drag and drop**: drag an entry's pill onto another day to move it (its time is kept). In the Day view, drag it to another hour to change its time. Press Esc to cancel a drag; double-click a pill to edit it.
- **Multiple entries per day**, each with a time. You can change an entry's date and time when you edit it.
- **WYSIWYG rich text editor** (Markdown underneath): type in a single page where bold looks bold, headings are large and the Markdown marks disappear. The marks show on the line your cursor is on, or everywhere with "Show formatting marks". A toolbar offers bold, italic, strikethrough, code, three heading sizes, bulleted and numbered lists and quotes; `Ctrl+B` and `Ctrl+I` work too.
- **Pictures** appear in the page where you put them. Add them with the **Image** button or by dropping a file onto the editor, and remove them with the button under each one. Large images are shrunk (longest side 1600 px) and stored inside the encrypted diary.
- **Categories** with their own colours. Add, rename, recolour and delete them from the Categories window. Renaming a category updates the entries that use it.
- **Search** across all entries from the box in the header; click a result to jump to its day.
- **Password protection.** The diary is encrypted with a password you choose, and the app asks for it every time it starts. **Lock** locks it immediately; **Settings (cog) → Change password** changes it.
- **Auto-lock** after a period of inactivity (default 10 minutes; change or turn off under **Settings → Auto-lock**). An entry you're in the middle of writing survives locking.
- **Backup**: **Settings → Back up encrypted copy** saves a copy of the encrypted file wherever you like.
- **Popup windows** for creating and editing entries and for managing categories; they are draggable and resizable.
- **Light and dark mode** that follow your Windows system setting.

## Using the app

| Action | How |
| --- | --- |
| Select a day | Click it on the calendar |
| New entry | Double-click a day, or use **New entry** / **Add entry for this day** |
| Edit or delete an entry | Use the buttons on its card in the day panel |
| Format text | Select text and use the toolbar buttons or shortcuts (click again to remove the format), or type Markdown such as `**bold**` or `# Heading` |
| Add a picture | **Image…** in the editor, or drop an image file onto the editor |
| Switch view | **Month / Week / Day** buttons above the calendar |
| Move around | **◀** / **▶** step a month, week or day (depending on the view); **Today** jumps back |
| Move an entry | Drag its pill onto another day (or, in the Day view, another hour) |
| Edit an entry quickly | Double-click its pill |
| Search | Type in the search box; clear it to go back to the day view |
| Manage categories | **Categories** button in the header (press Enter to apply a rename) |

Deleting a category does not delete its entries; they are shown in grey.

Entries are stored as Markdown, so you can also type the syntax yourself (`**bold**`, `# Heading`, `- list`, `> quote`, `` `code` ``).

## Running it

You need the [Rust toolchain](https://rustup.rs/). From the project folder:

```
cargo run
```

Use `cargo run --release` for an optimised build. The build embeds the app icon in the `.exe`, which needs the Windows SDK's resource compiler (installed with the Visual Studio C++ build tools). If it is missing, the build still succeeds with a warning and the file just keeps the default icon.

## Where your data is stored

Entries, categories, images and settings are saved, encrypted, to `entries.json` in the project folder (the folder is fixed when the app is built, so it doesn't depend on where you launch it from). The file is listed in `.gitignore`, so your diary is not committed to git.

## Encryption

- Your password is stretched into a key with **Argon2id** (64 MiB, 3 passes), and the diary is sealed with **XChaCha20-Poly1305**, which also detects a wrong password or a damaged or tampered file.
- `entries.json` is a small JSON envelope holding the key-derivation settings, a random salt and nonce, and the ciphertext. Nothing in it reveals your entries or pictures.
- Every save uses a fresh nonce, is checked by decrypting it again, and replaces the old file atomically.
- **There is no password recovery.** If you forget the password, the diary can't be read.
- Passwords must be at least 8 characters.
- An unencrypted `entries.json` from an earlier version is offered for encryption the first time you run the app. It is replaced by the encrypted version and no plain-text backup is kept.
- Text and pictures held in memory while the app is unlocked are not protected. Use **Lock** if you step away.
- Because pictures are inside the file, a diary with many large pictures makes every save slower and the file bigger.

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
      "text": "# A good day\n\nSome **bold** thoughts.\n\n![photo](img:i1760000000000)"
    }
  ],
  "images": { "i1760000000000": "<base64 JPEG or PNG>" },
  "settings": { "auto_lock_minutes": 10 }
}
```

## Project layout

- `src/main.rs` contains the application: the UI, lock screen, editor and file handling.
- `src/markdown.rs` parses and renders entries for display and holds the toolbar's text-formatting logic.
- `src/live.rs` styles the text as you type (the WYSIWYG layout) and splits an entry into text and picture blocks.
- `src/vault.rs` contains the password-based encryption.
- `src/icon.rs` draws the app icon; `build.rs` turns it into the `.exe` icon.
- `Cargo.toml` lists the dependencies: `eframe`/`egui_extras` (UI), `chrono`/`jiff` (dates), `serde`/`serde_json`, `pulldown-cmark` (Markdown), `image` (pictures), `rfd` (file dialogs), `argon2`/`chacha20poly1305`/`base64`/`getrandom` (encryption), and `winresource` (build only).
