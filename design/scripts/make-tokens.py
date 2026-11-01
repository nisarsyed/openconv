"""Build design/tokens.json (W3C Design Tokens format) from the canvas stylesheet, so no value is retyped.

Usage: python3 make-tokens.py <canvas>/project/r2/oc.css design/tokens.json
"""
import json, re, sys, pathlib
css = pathlib.Path(sys.argv[1]).read_text()

def block(selector):
    m = re.search(re.escape(selector) + r'\s*\{(.*?)\}', css, re.S)
    return dict(re.findall(r'--([\w-]+):\s*([^;]+);', m.group(1)))

def hex8(v):
    v = v.strip()
    m = re.match(r'rgba\((\d+),\s*(\d+),\s*(\d+),\s*([\d.]+)\)', v)
    if m:
        r, g, b, a = int(m[1]), int(m[2]), int(m[3]), float(m[4])
        return f'#{r:02X}{g:02X}{b:02X}{round(a * 255):02X}'
    return v.upper()

def color(v, desc=None):
    t = {'$type': 'color', '$value': hex8(v)}
    if desc: t['$description'] = desc
    return t

light, dark = block('.oc, .oc.light'), block('.oc.dark')
USE = {
    'desk': 'Behind windows in mockups only; not used by the app.',
    'window': 'Transcript ground.', 'sidebar-bg': 'Conversation list and the Settings sidebar.',
    'panel': 'Right-hand panel, banners.', 'elevated': 'Cards, grouped rows, dialogs.',
    'fill': 'Segmented controls, a disabled message field.', 'fill-hover': 'Hover.',
    'field': 'Text fields.', 'text': 'Primary text.', 'text-2': 'Secondary text, notices.', 'text-3': 'Tertiary text, the offline dot.',
    'line': 'Hairlines, notice rules.', 'line-strong': 'Top border of the message bar and the account bar; control borders.',
    'warn-text': 'Away-too-long bar and banner text.', 'warn-bg': 'Its ground.',
    'danger': 'Not sent; destructive text.', 'online': 'The connected dot.',
}
def palette(src):
    out = {}
    for k, desc in USE.items():
        if k == 'desk': continue
        out['sidebar' if k == 'sidebar-bg' else k] = color(src[k], desc)
    return out

def avatars(src):
    return {str(i): {'background': color(src[f'av-{i}']), 'initials': color(src[f'av-{i}-t'])} for i in range(1, 7)}

accents = {}
for name in ('teal', 'ink', 'persimmon', 'moss'):
    l, d = block(f'.oc.accent-{name}'), block(f'.oc.dark.accent-{name}')
    accents[name] = {
        'accent': color(l['accent'], 'Fills selection, filled buttons, badges, the jump pill. White text on it passes 4.5:1.'),
        'text': {'light': color(l['accent-text']), 'dark': color(d['accent-text'])},
        'tint': {'light': color(l['accent-tint']), 'dark': color(d['accent-tint'])},
    }

base = block('.oc')
def typo(v):
    w, size_line, fam = re.match(r'(\d+)\s+(\S+)\s+var\(--(\w[\w-]*)\)', v).groups()
    size, line = size_line.split('/')
    return {'$type': 'typography', '$value': {'fontFamily': '{font.display}' if fam == 'font-display' else '{font.text}',
                                              'fontWeight': int(w), 'fontSize': size, 'lineHeight': line}}
TYPE = ['large', 'title1', 'title2', 'title3', 'headline', 'body', 'callout', 'caption', 'caption-strong']
def sizes(sel):
    b = block(sel)
    return {k[2:]: {'fontSize': re.search(r'\s(\d+px)/', v).group(1), 'lineHeight': re.search(r'/(\d+px)', v).group(1)}
            for k, v in b.items() if k.startswith('t-')}

def dim(v): return {'$type': 'dimension', '$value': v.strip()}
sb_dark = re.search(r'\.oc\.sb-dark \.oc-sidebar \{(.*?)\}', css, re.S).group(1)
sbd = dict(re.findall(r'--([\w-]+):\s*([^;]+);', sb_dark))
sbd_bg = re.search(r'background:\s*(#\w+)', sb_dark).group(1)

tokens = {
    '$description': 'OpenConv design tokens, agreed in #133. Generated from the design canvas stylesheet; see planning/design.md for how each is used. Colours with alpha are #RRGGBBAA.',
    'font': {
        'text': {'$type': 'fontFamily', '$value': ['SF Pro Text', '-apple-system'], '$description': 'The system font.'},
        'display': {'$type': 'fontFamily', '$value': ['SF Pro Display', '-apple-system'], '$description': 'Titles.'},
        'mono': {'$type': 'fontFamily', '$value': ['SF Mono', 'ui-monospace'], '$description': 'Join codes and relay addresses.'},
    },
    'type': {k: typo(base['t-' + k]) for k in TYPE},
    'space': {str(int(base[k][:-2])): dim(base[k]) for k in ('s1', 's2', 's3', 's4', 's5', 's6', 's8')},
    'radius': {'control': dim(base['r-control']), 'row': dim(base['r-row']), 'card': dim(base['r-card']),
               'window': dim(base['r-window']), 'pill': dim('999px'), 'conversation-avatar': dim('9px')},
    'size': {'bar': dict(dim(base['bar']), **{'$description': 'Header, message bar and account bar: one height, so their borders line up.'}),
             'sidebar': dim(base['sidebar']), 'panel': dim(base['inspector']), 'settings-sidebar': dim('210px'),
             'avatar': dim(base['avatar']), 'avatar-small': dim('24px'), 'avatar-large': dim('44px')},
    'color': {
        'light': palette(light), 'dark': palette(dark),
        'busy': color('#C98A12', 'The connecting and catching-up dot, both appearances.'),
        'destructive-fill': color('#C2271D', 'Filled destructive buttons, white text, both appearances.'),
    },
    'accent': accents,
    'avatar': {'$description': 'Chosen from the member\'s key, never their name.', 'light': avatars(light), 'dark': avatars(dark)},
    'settings': {
        '$description': 'What the Appearance settings change. Defaults are the tokens above.',
        'density': {'compact': {'bar': dim(block('.oc.d-compact')['bar']), 'avatar': dim(block('.oc.d-compact')['avatar'])}},
        'text-size': {'$description': 'Font size and line height per text style.', 'smaller': sizes('.oc.ts-small'), 'larger': sizes('.oc.ts-large')},
        'typeface': {'rounded': {'$description': 'SF Pro Rounded for all text.'}, 'serif-titles': {'$description': 'New York for display styles only.'}},
        'sidebar': {'tinted': {'$description': 'Sidebar ground is the accent mixed 8% into the sidebar colour.'},
                    'dark': {'background': color(sbd_bg), 'text': color(sbd['text']), 'text-2': color(sbd['text-2']),
                             'fill': color(sbd['fill']), 'line': color(sbd['line']), 'line-strong': color(sbd['line-strong']),
                             'selected-row': color('rgba(255,255,255,0.11)')}},
        'messages': {'compact': {'name-column': dim('128px'), '$description': 'No avatars; time and name in a right-aligned column.'},
                     'highlight-mine': {'$description': 'Your messages on the accent tint.'}},
        'buttons': {'$description': 'Primary buttons: tinted (default; accent tint ground, accent text), filled (accent ground, white text), outlined (accent border, accent text).'},
    },
}
json.dump(tokens, open(sys.argv[2], 'w'), indent=2, ensure_ascii=False)
open(sys.argv[2], 'a').write('\n')
