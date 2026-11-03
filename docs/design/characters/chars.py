# Generates original agent characters as inline SVG.
SHAPES = {
 'round': dict(body='<ellipse cx="60" cy="70" rx="38" ry="35"/>', eye_y=62, top=35, belly=(60,84,22,14), feet_y=104, arm_y=76, arm_dx=40),
 'bean':  dict(body='<rect x="28" y="24" width="64" height="80" rx="32"/>', eye_y=54, top=24, belly=(60,82,20,15), feet_y=104, arm_y=72, arm_dx=34),
 'block': dict(body='<rect x="25" y="34" width="70" height="70" rx="24"/>', eye_y=60, top=34, belly=(60,84,22,13), feet_y=104, arm_y=76, arm_dx=37),
}
INK = '#0F1011'

def accessory(kind, s, state, dark=INK):
    if kind == 'pen':
        cls = 'acc pen' + (' scribble' if state == 'working' else '')
        return f'<g transform="translate({60+s["arm_dx"]-2} {s["arm_y"]-18}) rotate(28)"><g class="{cls}"><rect x="-3.5" y="0" width="7" height="30" rx="2" fill="#ECEDEE"/><rect x="-3.5" y="0" width="7" height="6" rx="2" fill="#8FA6FF"/><path d="M-3.5 30 L0 38 L3.5 30 Z" fill="#ECEDEE"/><path d="M-1.2 34.5 L0 38 L1.2 34.5 Z" fill="{INK}"/></g></g>'
    if kind == 'lens':
        t = s['top']
        cls = 'acc lens' + (' scan' if state == 'working' else '')
        return f'<g class="{cls}"><path d="M60 {t+2} C60 {t-10} 66 {t-14} 70 {t-18}" fill="none" stroke="{dark}" stroke-width="4" stroke-linecap="round"/><circle cx="72" cy="{t-21}" r="7.5" fill="#ECEDEE"/><circle cx="72" cy="{t-21}" r="4.5" fill="#8FA6FF"/><circle cx="70.5" cy="{t-22.5}" r="1.6" fill="#fff"/></g>'
    if kind == 'book':
        cls = 'acc book' + (' flip' if state == 'working' else '')
        return f'<g transform="translate(60 {s["arm_y"]+12})"><g class="{cls}"><rect x="-17" y="-11" width="34" height="22" rx="3" fill="#ECEDEE"/><rect x="-1.2" y="-11" width="2.4" height="22" fill="#C9BDFB"/><path d="M-12 -4 H-5 M-12 1 H-5 M5 -4 H12 M5 1 H12" stroke="#8B929B" stroke-width="1.6" stroke-linecap="round"/></g></g>'
    return ''

def eyes(s, state, ink=INK):
    y = s['eye_y']
    if state == 'asleep':
        return f'<g fill="none" stroke="{ink}" stroke-width="3" stroke-linecap="round"><path d="M42 {y} q6 5 12 0"/><path d="M66 {y} q6 5 12 0"/></g>'
    if state == 'incident':
        return f'<g fill="none" stroke="{ink}" stroke-width="2.6" stroke-linecap="round"><path d="M44 {y-5} l8 8 m0 -8 l-8 8"/><path d="M68 {y-5} l8 8 m0 -8 l-8 8"/></g>'
    pup = ' look' if state == 'working' else ''
    up = -2 if state == 'needs' else 0
    return (f'<g class="eyes"><ellipse cx="48" cy="{y}" rx="9" ry="10" fill="#fff"/><ellipse cx="72" cy="{y}" rx="9" ry="10" fill="#fff"/>'
            f'<g class="pupils{pup}"><circle cx="49" cy="{y+1+up}" r="4.6" fill="{ink}"/><circle cx="73" cy="{y+1+up}" r="4.6" fill="{ink}"/>'
            f'<circle cx="50.5" cy="{y-1+up}" r="1.4" fill="#fff"/><circle cx="74.5" cy="{y-1+up}" r="1.4" fill="#fff"/></g></g>')

def mouth(s, state):
    y = s['eye_y'] + 14
    if state == 'asleep': return f'<ellipse cx="60" cy="{y+1}" rx="3" ry="2.2" fill="{INK}" fill-opacity=".6"/>'
    if state == 'incident': return f'<path d="M53 {y+3} q3.5 -4 7 0 q3.5 4 7 0" fill="none" stroke="{INK}" stroke-width="2.4" stroke-linecap="round"/>'
    if state == 'needs': return f'<ellipse cx="60" cy="{y+1}" rx="4" ry="3.6" fill="{INK}"/>'
    return f'<path d="M54 {y} q6 5 12 0" fill="none" stroke="{INK}" stroke-width="2.6" stroke-linecap="round"/>'

def arms(s, color_dark, state):
    y, dx = s['arm_y'], s['arm_dx']
    left = f'<ellipse cx="{60-dx}" cy="{y}" rx="6" ry="10" fill="{color_dark}" transform="rotate(18 {60-dx} {y})"/>'
    if state == 'needs':
        right = f'<g class="wave" style="transform-origin:{60+dx-6}px {y-6}px;transform-box:view-box"><ellipse cx="{60+dx-3}" cy="{y-17}" rx="6" ry="12" fill="{color_dark}" transform="rotate(22 {60+dx-3} {y-17})"/></g>'
    else:
        right = f'<ellipse cx="{60+dx}" cy="{y}" rx="6" ry="10" fill="{color_dark}" transform="rotate(-18 {60+dx} {y})"/>'
    return left + right

def overlay(s, state):
    t = s['top']
    if state == 'needs':
        return f'<g class="bubble"><rect x="86" y="{t-24}" width="26" height="22" rx="8" fill="#F5B353"/><path d="M92 {t-3} l-4 7 l9 -7 Z" fill="#F5B353"/><text x="99" y="{t-8}" text-anchor="middle" font-family="Inter, sans-serif" font-size="14" font-weight="700" fill="#2A1404">!</text></g>'
    if state == 'asleep':
        return '<g class="zzz" font-family="Inter, sans-serif" font-weight="700" fill="#A3A9B1"><text class="z1" x="88" y="34" font-size="12">z</text><text class="z2" x="97" y="22" font-size="15">z</text></g>'
    if state == 'incident':
        st = '<path d="M0 -5 L1.5 -1.5 L5 0 L1.5 1.5 L0 5 L-1.5 1.5 L-5 0 L-1.5 -1.5 Z" fill="#F5B353"/>'
        return f'<g class="stars" style="transform-origin:60px {t-8}px"><g transform="translate(40 {t-8})">{st}</g><g transform="translate(80 {t-10})">{st}</g><g transform="translate(60 {t-16}) scale(.7)">{st}</g></g>'
    return ''

def character(color, dark, shape, acc, state='idle', size=120, label=None, hatch=False):
    s = SHAPES[shape]
    bx, by, brx, bry = s['belly']
    lens_behind = acc == 'lens'
    body_group = (
        f'<g class="body">'
        + (accessory(acc, s, state, dark) if lens_behind else '')
        + f'<g fill="{dark}"><ellipse cx="47" cy="{s["feet_y"]}" rx="9" ry="5"/><ellipse cx="73" cy="{s["feet_y"]}" rx="9" ry="5"/></g>'
        + arms(s, dark, state)
        + f'<g fill="{color}">{s["body"]}</g>'
        + f'<ellipse cx="{bx}" cy="{by}" rx="{brx}" ry="{bry}" fill="#fff" fill-opacity=".18"/>'
        + eyes(s, state) + mouth(s, state)
        + ('' if lens_behind else accessory(acc, s, state, dark))
        + '</g>')
    shadow = '<ellipse class="shadow" cx="60" cy="111" rx="30" ry="4" fill="#000" fill-opacity=".35"/>'
    aria = label or 'Agent character'
    cls = f'ch st-{state}' + (' hatch' if hatch else '')
    return (f'<svg class="{cls}" role="img" aria-label="{aria}" width="{size}" height="{size}" viewBox="0 0 120 120" style="overflow: visible; flex: 0 0 auto">'
            f'{shadow}{body_group}{overlay(s, state)}</svg>')

CSS = """
.ch .body,.ch .eyes,.ch .pupils,.ch .acc,.ch .wave,.ch .bubble,.ch .zzz text,.ch .stars,.ch .shadow{transform-box:fill-box;transform-origin:center bottom}
.ch .eyes{transform-origin:center}
.st-idle .body{animation:breathe 3.2s ease-in-out infinite}
.st-idle .eyes,.st-working .eyes,.st-needs .eyes{animation:blink 4.6s infinite}
.st-working .body{animation:bob .7s ease-in-out infinite}
.st-working .pupils.look{animation:look 2.4s ease-in-out infinite;transform-box:view-box;transform-origin:0 0}
.acc.scribble{animation:scribble .35s ease-in-out infinite alternate;transform-origin:center top}
.acc.scan{animation:scan 2.4s ease-in-out infinite;transform-origin:center bottom}
.acc.flip{animation:flip 1.2s ease-in-out infinite}
.st-needs .body{animation:hop 1.6s ease-in-out infinite}
.ch .wave{animation:wave .9s ease-in-out infinite alternate}
.ch .bubble{animation:pulse 1.6s ease-in-out infinite}
.st-incident .body{animation:wobble 1.4s ease-in-out infinite;transform-origin:center bottom}
.ch .stars{animation:spin 2.2s linear infinite;transform-box:view-box}
.st-asleep .body{animation:breathe 4.4s ease-in-out infinite}
.ch .zzz .z1{animation:float 2.8s ease-in infinite}
.ch .zzz .z2{animation:float 2.8s ease-in 1.4s infinite}
.ch.hatch .body{animation:hatch 5s cubic-bezier(.3,1.4,.5,1) infinite, breathe 3.2s ease-in-out 1s infinite}
.ch.hatch .shadow{animation:shadow 5s ease-out infinite}
@keyframes breathe{0%,100%{transform:scale(1,1)}50%{transform:scale(1.025,.975)}}
@keyframes blink{0%,92%,100%{transform:scaleY(1)}95%{transform:scaleY(.1)}}
@keyframes bob{0%,100%{transform:translateY(0)}50%{transform:translateY(-3px)}}
@keyframes look{0%,100%{transform:translateX(0)}30%{transform:translateX(-3px)}70%{transform:translateX(3px)}}
@keyframes scribble{from{transform:rotate(-6deg)}to{transform:rotate(6deg)}}
@keyframes scan{0%,100%{transform:rotate(-8deg)}50%{transform:rotate(10deg)}}
@keyframes flip{0%,100%{transform:translateY(0) scaleX(1)}50%{transform:translateY(-2px) scaleX(.92)}}
@keyframes hop{0%,60%,100%{transform:translateY(0)}70%{transform:translateY(-6px)}80%{transform:translateY(0)}}
@keyframes wave{from{transform:rotate(-14deg)}to{transform:rotate(16deg)}}
@keyframes pulse{0%,100%{transform:scale(1)}50%{transform:scale(1.12)}}
@keyframes wobble{0%,100%{transform:rotate(-4deg)}50%{transform:rotate(4deg)}}
@keyframes spin{to{transform:rotate(360deg)}}
@keyframes float{0%{transform:translate(0,4px);opacity:0}20%{opacity:1}100%{transform:translate(6px,-14px);opacity:0}}
@keyframes hatch{0%{transform:scale(.15) translateY(20px);opacity:0}12%{transform:scale(1.12,.9);opacity:1}18%{transform:scale(.94,1.06)}24%,100%{transform:scale(1)}}
@keyframes shadow{0%{opacity:0}12%,100%{opacity:1}}
.spark{animation:spark 5s ease-out infinite;transform-box:fill-box;transform-origin:center}
@keyframes spark{0%,8%{opacity:0;transform:scale(.2)}14%{opacity:1;transform:scale(1)}30%,100%{opacity:0;transform:scale(1.4)}}
@media (prefers-reduced-motion:reduce){.ch *,.spark{animation:none !important}}
"""
