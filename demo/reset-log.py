"""Remove only demo log state while the services using it are stopped."""
from pathlib import Path
import shutil

for directory in (Path('/reset/log'), Path('/reset/proxy'), Path('/reset/ui')):
    for entry in directory.iterdir():
        if entry.is_dir() and not entry.is_symlink():
            shutil.rmtree(entry)
        else:
            entry.unlink()

Path('/reset/control/tamper').unlink(missing_ok=True)
print('ログ、Proxy・UI のスナップショット、tamper を初期化しました。')
