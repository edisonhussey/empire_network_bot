# Storage usage (checked 2026-10-17)

Only aggregate commands were used (`df`, `diskutil apfs list`, `du -sk`, `ls`, `lsof` names only). Nothing was opened or deleted.

## Disk
- Internal container: 245 GB, about 233 GB used, about 12.6 GB free (95%).
- Data volume: 199.7 GB. System: 12.6 GB. Preboot: 9.0 GB. VM: 9.7 GB. Recovery: 1.3 GB.
- No Time Machine local snapshots (`tmutil listlocalsnapshots /` is empty), so snapshots are not the cause.

## Why "macOS + System Data" shows about 100 GB
The cause is one folder:

`/private/var/folders/1l/c9ffpdqx0n57kyzlc7vc33fh0000gn/X/com.google.Chrome.code_sign_clone/`

- It holds 44 `code_sign_clone.*` folders of about 2.09 GB each, about 85.9 GB in total.
- Each one is a copy of Chrome.app (1.41 GB installed, about 2.09 GB per copy as `du` counts it).
- macOS makes one each time Chrome is launched and, if the launch is killed, crashes or is automated, it never cleans it up.
- Only 1 of the 44 is held open by a running process. The other 43 look orphaned.
- Most are dated Oct 12, 10:23 to 15:35. That points to many Chrome launches in one day, probably from browser automation or Claude-in-Chrome.
- The oldest dates from Sep 23.
- macOS counts `/private/var/folders` as System Data, so about 86 GB lands there. Add 12.6 GB system, 9 GB Preboot and 9.7 GB VM, and the total is about 100 GB.
- Clones share blocks with the original app until Chrome updates. Chrome was last updated Oct 15, 22:11. Clones made before that point to the older version and likely hold their own unique blocks.
- `du` adds up to about 251 GB against 199.7 GB actually allocated, so some of the 86 GB is shared and double-counted. Real reclaimable space is probably well under 86 GB, but it is likely tens of GB. Actual space freed has to be measured after cleanup.

## Why "Documents" shows 92 GB
- The real `~/Documents` folder is 0.4 GB.
- macOS's "Documents" category is not that folder. It groups most user files outside Library, Apps and media.
- The pieces I summed come to about 84 GB (the Movies, Music and Pictures folders add roughly 3 GB more, close to the 92 GB shown):
  - ~/Desktop: 50.7 GB
  - ~/Downloads: 6.3 GB
  - Hidden dev folders in the home directory: about 27 GB
- The hidden dev folders are `.ghcup` 7.9, `.ollama` 3.1, `.vscode` 2.6, `.cache` 2.5, `.stack` 2.1, `.rustup` 1.3, `.codex` 1.1, `.cabal` 1.0, `.gradle` 1.0, `.cargo` 0.9, `.npm` 0.7 and others.
- Biggest Desktop folders: open_media 8.6, open_edit 6.7, tau_quantitative 5.5, custom_image 5.1, github_gge_bot 4.9, automated_editing 4.3, old_good_projects 4.1, tau_fund 3.2, BACKUP_DONT_TOUCH 2.2, outdated_jav_app 2.0.

## Why "Applications" shows about 40 GB
- /Applications is 15.8 GB (a second ~/Applications folder holds 0.7 GB).
- macOS adds the app data tied to those apps:
  - ~/Library/Application Support: 19.7 GB
  - ~/Library/Containers: 7.1 GB
  - ~/Library/Group Containers: 1.6 GB
- Together that is about 44.9 GB, close to the roughly 40 GB shown.
- ~/Library/Caches is a separate 11.1 GB and is probably counted elsewhere.

## Other
- ~/Library is 42 GB in total.
- /opt is 4.9 GB and /usr/local is 2.9 GB.
- /private/var/db is 2.4 GB and /private/var/vm is 2.0 GB.

## Suggested cleanup (not done, your call)
1. Quit Chrome fully. Then the 43 unused folders in `.../X/com.google.Chrome.code_sign_clone/` can be removed. A restart also clears most of it. Check the free-space change before deleting more.
2. If it refills, find what launches Chrome repeatedly (automation, MCP browser tools, test scripts).
3. Review the Desktop project folders and dev caches (`.ghcup`, `.ollama`, `.cache`, `.stack`) if you need more room.

## Cleanup done (2026-10-17)
- Removed 43 of 44 Chrome `code_sign_clone.*` folders. Kept `code_sign_clone.zNoHqT` (newest). Chrome was fully closed and no clone was in use.
- Removed `~/.ghcup` (7.9 GB, Haskell toolchain), `~/.ollama` (3.1 GB, models) and `~/.cache` (about 2.5 GB, re-downloadable caches).
- Free space went from 12.6 GB to 35.4 GB (container 85.5% used, down from 95%), about 22.8 GB reclaimed. That is lower than the 86 GB the clones appeared to take because most of their blocks were shared with the real Chrome.app.
- Not touched: `~/.stack` (2.1 GB) and `~/.cabal` (1.0 GB) are also Haskell. `Ollama.app` is still installed and its server was running.

## Second pass (2026-10-17)
Deleted: `Ollama.app` and its ~/Library data, `~/.stack`, `~/.cabal`. Also removed the dead ghcup hook line from `~/.zshrc` (backup in the Claude scratchpad as `zshrc.bak`).

State after cleanup: Data volume 176.5 GB (was 199.7), container free 40 GB (was 12.6). `/private/var/folders` is 4.3 GB (was 86.7).

### What "macOS + System Data" is made of now
System Data is everything macOS can't file under Apps, Documents, Photos and so on, plus the OS itself:

| Item | Size |
|---|---|
| System volume (macOS itself) | 12.6 GB |
| Preboot (boot files, cryptexes) | 9.0 GB |
| VM / swap volume (swap in use 4.6 GB of 5 GB) | 5.4 GB |
| Recovery | 1.3 GB |
| sleepimage (`/private/var/vm`) | 2.0 GB |
| ~/Library/Caches (Google 4.2, pip 1.3, cpptools 1.3, Telegram 0.9 ...) | 11.1 GB |
| ~/Library/Containers/com.apple.mediaanalysisd (Photos/media analysis) | 5.5 GB |
| /Library (Developer 2.4, Updates 0.7 ...) | 6.5 GB |
| /opt + /usr/local (Homebrew etc.) | 7.8 GB |
| /private/var (db 2.4, folders 4.3 ...) | 8.9 GB |

That is about 70 GB of readable items, and ~26 GB more sits in root-only paths that `du` could not open without sudo or Full Disk Access (Data volume 176.5 GB vs ~150 GB of visible folders). No APFS snapshots, no Time Machine snapshots, no deleted-but-open files exist, so those are ruled out.

### The 90 GB figure is probably out of date
- System Settings > Storage recalculates lazily. It often keeps showing the old number until you reopen the panel after several minutes or restart.
- It also counts purgeable and cache data inside System Data.
- To see the 26 GB for yourself, run: `sudo du -xhd1 /System/Volumes/Data | sort -h | tail`. Then repeat on the biggest folder.

## Third pass (2026-10-17)
Deleted `~/Library/Caches/pip` (1.3 GB) and `~/Library/Caches/vscode-cpptools` (1.3 GB). Both are rebuilt on demand.
Skipped `~/Library/Caches/Google/Chrome` (4.2 GB, the browser cache) because Chrome was running. It can be deleted after quitting Chrome.
