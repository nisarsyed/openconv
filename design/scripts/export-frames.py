"""Render every agreed frame of the canvas, light and dark, to PNG with headless Chrome.

Usage: python3 export-frames.py <canvas>/project/r2 design/frames
"""
import json, subprocess, pathlib, sys
here = pathlib.Path(__file__).parent
src = pathlib.Path(sys.argv[1])
out = pathlib.Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome'
FRAMES = [  # (board, name, w, h)
    ('Styles.dc.html', 'styles', 1440, 1040), ('Components.dc.html', 'components', 1440, 1040),
    ('Main.dc.html', 'conversation', 1100, 720), ('Requests.dc.html', 'join-requests', 1100, 720),
    ('Members.dc.html', 'members', 1100, 720), ('FirstRun.dc.html', 'first-run', 1100, 720),
    ('Empty.dc.html', 'no-conversations', 1100, 720), ('Host.dc.html', 'started', 1100, 720),
    ('Join.dc.html', 'join-with-code', 1100, 720), ('Waiting.dc.html', 'waiting', 1100, 720),
    ('Settings.dc.html', 'settings-appearance', 1100, 800), ('SettingsSidebar.dc.html', 'settings-privacy', 1100, 720),
    ('r3-Offline.dc.html', 'offline', 1100, 720), ('r3-CatchingUp.dc.html', 'catching-up', 1100, 720),
    ('r3-RelayError.dc.html', 'cant-reach-server', 1100, 720), ('r3-States.dc.html', 'states', 1440, 820),
    ('r3-Stranded.dc.html', 'away-too-long', 1100, 720), ('r3-Leaving.dc.html', 'leaving', 1100, 720),
    ('r3-Removed.dc.html', 'removed', 1100, 720), ('r3-Delete.dc.html', 'delete-conversation', 1100, 720),
    ('r3-Info.dc.html', 'conversation-info', 1100, 720), ('r3-Unnamed.dc.html', 'naming', 1100, 720),
    ('r3-Storage.dc.html', 'settings-storage', 1100, 720),
]
for board, name, w, h in FRAMES:
    for mode in ('light', 'dark'):
        tmp = src / f'_export_{name}.html'
        subprocess.run(['node', str(here / 'render-board.mjs'), str(src / board), json.dumps({'appearance': mode, 'accent': 'teal'}), str(tmp)], check=True)
        png = out / f'{name}-{mode}.png'
        subprocess.run([chrome, '--headless=new', '--disable-gpu', '--hide-scrollbars', f'--window-size={w},{h}', f'--screenshot={png}', tmp.as_uri()], capture_output=True, timeout=60, check=True)
        tmp.unlink()
print(len(list(out.glob('*.png'))))
