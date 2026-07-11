#!/usr/bin/env python3
"""Generate a flat, net-label-wired KiCad 10 schematic from LCSC-pulled symbols.

Connectivity is by global label: every used pin gets a global label placed at
its connection point, so pins sharing a net name are electrically joined.
"""
import re, uuid, sys, math

LIB = "lib/dongle-lite.kicad_sym"
LIBNAME = "dongle-lite"

KICAD_POWER_LIB = "/Applications/KiCad/KiCad.app/Contents/SharedSupport/symbols/power.kicad_sym"

def _extract_pwr_flag():
    """Pull the known-good PWR_FLAG symbol block verbatim from KiCad's power lib."""
    lines = open(KICAD_POWER_LIB).read().splitlines()
    out, depth, capturing = [], 0, False
    for l in lines:
        if not capturing and l.startswith('\t(symbol "PWR_FLAG"'):
            capturing = True
        if capturing:
            out.append(l)
            depth += l.count("(") - l.count(")")
            if depth == 0:
                break
    return out

PWR_FLAG_BLOCK = _extract_pwr_flag()

def uid(): return str(uuid.uuid4())

def _extract_power(name):
    """Pull a power/flag symbol block verbatim from KiCad's power lib."""
    lines = open(KICAD_POWER_LIB).read().splitlines()
    out, depth, cap = [], 0, False
    for l in lines:
        if not cap and l.startswith(f'\t(symbol "{name}"'):
            cap = True
        if cap:
            out.append(l); depth += l.count("(") - l.count(")")
            if depth == 0: break
    return out

def load_symbols(path):
    """Return {name: {'block': lines, 'pins': {num: (x,y,angle,length)}}}."""
    lines = open(path).read().splitlines()
    idx = [i for i,l in enumerate(lines) if re.match(r'^  \(symbol "', l)]
    idx.append(len(lines))
    syms = {}
    for a,b in zip(idx, idx[1:]):
        name = lines[a].split('"')[1]
        block = lines[a:b]
        # the final block runs to EOF and captures the library's own closing
        # ")" (indent 0); strip any such trailing top-level closer(s)
        while block and block[-1] == ")":
            block.pop()
        text = "\n".join(block)
        fpm = re.search(r'"Footprint"\s*"([^"]*)"', text)
        mpnm = re.search(r'"MPN"\s*"([^"]*)"', text)
        lcscm = re.search(r'"LCSC Part"\s*"([^"]*)"', text)
        pins = {}
        # each pin: (at x y angle) (length L) ... (number "N"
        for m in re.finditer(
            r'\(pin \w+ \w+\s*\(at ([-\d.]+) ([-\d.]+) ([-\d.]+)\)\s*\(length ([-\d.]+)\).*?\(number "([^"]+)"',
            text, re.S):
            x,y,ang,ln,num = m.groups()
            pins[num] = (float(x), float(y), float(ang), float(ln))
        syms[name] = {"block": block, "pins": pins,
                      "fp": fpm.group(1) if fpm else "",
                      "mpn": mpnm.group(1) if mpnm else name,
                      "lcsc": lcscm.group(1) if lcscm else ""}
    syms["PWR_FLAG"] = {"block": PWR_FLAG_BLOCK, "pins": {"1": (0.0, 0.0, 90.0, 0.0)},
                        "fp": "", "mpn": "PWR_FLAG", "lcsc": ""}
    # stock KiCad power-port symbols (pin at origin), embedded so rails render as
    # power ports instead of text labels
    for pn in ("GND", "+3V3", "+5V", "+1V2", "+1V1"):
        syms[pn] = {"block": _extract_power(pn), "pins": {"1": (0.0, 0.0, 90.0, 0.0)},
                    "fp": "", "mpn": pn, "lcsc": ""}
    return syms

def pin_abs(comp_at, pin):
    """Absolute connection point of a pin for a component placed at comp_at=(X,Y,rot).
    Symbol space is Y-up; schematic is Y-down, so pin Y is negated. rot in {0,90,180,270}."""
    X,Y,rot = comp_at
    px,py,_,_ = pin
    # rotate (px, -py) by rot (schematic space)
    sx, sy = px, -py
    r = math.radians(rot)
    rx = sx*math.cos(r) - sy*math.sin(r)
    ry = sx*math.sin(r) + sy*math.cos(r)
    return (round(X+rx,2), round(Y+ry,2))

def lib_symbols_section(syms, used):
    out = ["\t(lib_symbols"]
    for name in used:
        block = syms[name]["block"]
        # rename header:  "  (symbol \"NAME\"" -> "\t\t(symbol \"LIBNAME:NAME\""
        first = block[0].replace(f'(symbol "{name}"', f'(symbol "{LIBNAME}:{name}"')
        out.append("\t\t"+first.strip())
        for l in block[1:]:
            out.append("\t\t"+l)
    out.append("\t)")
    return "\n".join(out)

def comp_instance(ref, name, at, value, footprint, root_uuid, fields, project="dongle-lite"):
    X,Y,rot = at
    props = [
        ('Reference', ref, 0, 3.0),
        ('Value', value, 0, -3.0),
        ('Footprint', footprint, 0, -5.0),
    ]
    s = [f'\t(symbol',
         f'\t\t(lib_id "{LIBNAME}:{name}")',
         f'\t\t(at {X} {Y} {rot})',
         f'\t\t(unit 1)',
         f'\t\t(exclude_from_sim no)(in_bom yes)(on_board yes)(dnp no)',
         f'\t\t(uuid "{uid()}")']
    for pn,pv,px,py in props:
        hide = "" if pn=="Reference" or pn=="Value" else " (hide yes)"
        s.append(f'\t\t(property "{pn}" "{pv}" (at {X} {round(Y+py,2)} 0) (effects (font (size 1.27 1.27)){hide}))')
    for k,v in fields.items():
        s.append(f'\t\t(property "{k}" "{v}" (at {X} {Y} 0) (effects (font (size 1.27 1.27)) (hide yes)))')
    s.append(f'\t\t(instances (project "{project}" (path "/{root_uuid}" (reference "{ref}") (unit 1))))')
    s.append('\t)')
    return "\n".join(s)

def glabel(net, at, angle=0):
    X,Y = at
    just = "left" if angle == 0 else "right"
    return (f'\t(global_label "{net}" (shape bidirectional) (at {X} {Y} {angle})\n'
            f'\t\t(effects (font (size 1.27 1.27)) (justify {just}))\n'
            f'\t\t(uuid "{uid()}"))')

def llabel(net, at):
    X,Y = at
    return (f'\t(label "{net}" (at {X} {Y} 0)\n'
            f'\t\t(effects (font (size 1.27 1.27)) (justify left))\n'
            f'\t\t(uuid "{uid()}"))')

def no_connect(at):
    X,Y = at
    return f'\t(no_connect (at {X} {Y}) (uuid "{uid()}"))'

def _emit_labels(body, c, syms, global_nets):
    pins = syms[c["sym"]]["pins"]
    nets = c["nets"]
    for num in nets:
        if num not in pins:
            print(f"WARN {c['ref']} pin {num} not in symbol {c['sym']}", file=sys.stderr)
    for num, pin in pins.items():
        net = nets.get(num)
        at = pin_abs(c["at"], pin)
        if net is None:
            body.append(no_connect(at))
        elif net in global_nets:
            body.append(glabel(net, at))
        else:
            body.append(llabel(net, at))

def wire(p1, p2):
    """Orthogonal L-wire between two exact points (endpoints must be pin coords)."""
    (x1,y1),(x2,y2) = p1, p2
    pts = [p1] + ([(x2,y1)] if x1!=x2 and y1!=y2 else []) + [p2]
    out = []
    for a,b in zip(pts, pts[1:]):
        if a==b: continue
        out.append(f'\t(wire (pts (xy {a[0]} {a[1]}) (xy {b[0]} {b[1]})) '
                   f'(stroke (width 0) (type default)) (uuid "{uid()}"))')
    return "\n".join(out)

def _snap(v, grid=1.27): return round(round(v/grid)*grid, 2)

def build_wired(components, out_path, power_nets, wires=None, auto_wire_dist=45.0,
                project="dongle-lite", paper="A2", force_label=()):
    """Single flat sheet. Rail nets -> power-port symbols; nearby 2-pin nets ->
    drawn wires; every other used pin -> a net label; None -> no-connect."""
    wires = list(wires or [])
    force_label = set(force_label)
    syms = load_symbols(LIB)
    for c in components:  # snap placement so pins land on the 1.27mm grid
        x,y,r = c["at"]; c["at"] = (_snap(x), _snap(y), r)
    pinabs = {}
    for c in components:
        for num,pin in syms[c["sym"]]["pins"].items():
            pinabs[(c["ref"],num)] = pin_abs(c["at"], pin)
    # auto-wire 2-pin signal nets whose pins are close together
    netpins = {}
    for c in components:
        for num,net in c["nets"].items():
            if net and net not in power_nets:
                netpins.setdefault(net, []).append((c["ref"], num))
    for net,ps in netpins.items():
        if len(ps) < 2 or net in force_label:
            continue
        pts = [pinabs[p] for p in ps]
        xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
        # wire a net only if all its pins sit in a small box (i.e. one cluster);
        # otherwise it crosses the sheet and stays a label
        if (max(xs)-min(xs)) + (max(ys)-min(ys)) <= auto_wire_dist:
            order = sorted(range(len(ps)), key=lambda i: (pts[i][0], pts[i][1]))
            for i in range(len(order)-1):        # daisy-chain the pins in order
                wires.append((ps[order[i]], ps[order[i+1]]))
    wired = set()
    for a,b in wires: wired.add(a); wired.add(b)
    # Breakout routing: every used pin gets a stub perpendicular to its IC edge
    # with a UNIQUE length within its (ref, side) row, so each net leaves on its
    # own channel. Wires then route between breakout points in open space and
    # never run along a pin row -- which is what used to short adjacent nets.
    OUT = {0.0:(-1,0), 180.0:(1,0), 90.0:(0,1), 270.0:(0,-1)}
    groups = {}
    for c in components:
        for num,pin in syms[c["sym"]]["pins"].items():
            if c["nets"].get(num) is None: continue
            if c["at"][2] != 0 or pin[2] not in OUT: continue   # only un-rotated
            side = pin[2]
            p = pinabs[(c["ref"],num)]
            perp = p[1] if side in (0.0,180.0) else p[0]
            groups.setdefault((c["ref"],side), []).append((perp, num))
    brk = {}
    for (ref,side),lst in groups.items():
        lst.sort()
        ox,oy = OUT[side]
        for i,(_,num) in enumerate(lst):
            L = 2.54 + 1.27*i
            p = pinabs[(ref,num)]
            brk[(ref,num)] = (round(p[0]+ox*L,2), round(p[1]+oy*L,2))
    used = sorted({c["sym"] for c in components} | set(power_nets.values()))
    root_uuid = uid()
    b = ["(kicad_sch", "\t(version 20260306)", '\t(generator "eeschema")',
         '\t(generator_version "10.0")', f'\t(uuid "{root_uuid}")', f'\t(paper "{paper}")',
         lib_symbols_section(syms, used)]
    for c in components:
        s = syms[c["sym"]]
        fields = dict(c.get("fields", {}))
        if s["lcsc"] and "LCSC" not in fields: fields["LCSC"] = s["lcsc"]
        b.append(comp_instance(c["ref"], c["sym"], c["at"], c.get("value") or s["mpn"],
                               c.get("fp") or s["fp"], root_uuid, fields, project))
        for num,pin in s["pins"].items():
            net = c["nets"].get(num)
            key = (c["ref"],num)
            at = pinabs[key]
            if net is None:
                b.append(no_connect(at)); continue
            P = brk.get(key)
            if P: b.append(wire(at, P))          # breakout stub
            if key in wired:
                continue                          # routed to its breakout below
            if P: at = P
            if net in power_nets:
                b.append(_pwr(power_nets[net], at, root_uuid, project))
            else:
                ang = 90 if pin[2] in (90.0, 270.0) else 0   # vertical on top/bottom pins
                b.append(glabel(net, at, ang))
    for a,bp in wires:
        if a in pinabs and bp in pinabs:
            b.append(wire(brk.get(a, pinabs[a]), brk.get(bp, pinabs[bp])))
        else:
            print(f"WARN wire endpoint missing: {a} {bp}", file=sys.stderr)
    b.append('\t(sheet_instances (path "/" (page "1")))')
    b.append('\t(embedded_fonts no)'); b.append(")")
    open(out_path,"w").write("\n".join(b)+"\n")
    return syms

_pwr_n = [0]
def _pwr(name, at, root_uuid, project):
    _pwr_n[0]+=1
    return comp_instance(f"#PWR0{_pwr_n[0]:02d}", name, (at[0],at[1],0), name, "",
                         root_uuid, {}, project)

def build_hier(sheets, project, out_root):
    """sheets: [{name, file, components}]. Nets on >1 sheet (or power) become
    global labels; single-sheet nets become local labels. Writes the root sheet
    to out_root and each child <file> beside it."""
    import os
    syms = load_symbols(LIB)
    # classify nets by how many sheets they touch
    net_sheets = {}
    for i,sh in enumerate(sheets):
        for c in sh["components"]:
            for net in c["nets"].values():
                if net: net_sheets.setdefault(net, set()).add(i)
    global_nets = {n for n,s in net_sheets.items() if len(s) > 1}
    root_uuid = uid()
    sheet_uuids = [uid() for _ in sheets]
    outdir = os.path.dirname(out_root)

    # ---- child sheets ----
    for i,sh in enumerate(sheets):
        used = sorted({c["sym"] for c in sh["components"]})
        b = ["(kicad_sch", "\t(version 20260306)", '\t(generator "eeschema")',
             '\t(generator_version "10.0")', f'\t(uuid "{uid()}")', '\t(paper "A3")',
             lib_symbols_section(syms, used)]
        for c in sh["components"]:
            s = syms[c["sym"]]
            fields = dict(c.get("fields", {}))
            if s["lcsc"] and "LCSC" not in fields: fields["LCSC"] = s["lcsc"]
            b.append(comp_instance(c["ref"], c["sym"], c["at"], c.get("value") or s["mpn"],
                                   c.get("fp") or s["fp"], sheet_uuids[i], fields, project))
            _emit_labels(b, c, syms, global_nets)
        b.append(f'\t(sheet_instances (path "/{sheet_uuids[i]}" (page "{i+2}")))')
        b.append('\t(embedded_fonts no)'); b.append(")")
        open(os.path.join(outdir, sh["file"]), "w").write("\n".join(b)+"\n")

    # ---- root sheet ----
    b = ["(kicad_sch", "\t(version 20260306)", '\t(generator "eeschema")',
         '\t(generator_version "10.0")', f'\t(uuid "{root_uuid}")', '\t(paper "A3")',
         "\t(lib_symbols)"]
    x = 30
    for i,sh in enumerate(sheets):
        b.append(f'''\t(sheet (at {x} 30) (size 45 35)
\t\t(stroke (width 0.1524) (type solid)) (fill (color 0 0 0 0.0))
\t\t(uuid "{sheet_uuids[i]}")
\t\t(property "Sheetname" "{sh['name']}" (at {x} 29.2 0) (effects (font (size 1.27 1.27)) (justify left bottom)))
\t\t(property "Sheetfile" "{sh['file']}" (at {x} 65.5 0) (effects (font (size 1.27 1.27)) (justify left top)))
\t\t(instances (project "{project}" (path "/{root_uuid}" (page "{i+2}")))))''')
        x += 55
    b.append('\t(sheet_instances (path "/" (page "1")))')
    b.append('\t(embedded_fonts no)'); b.append(")")
    open(out_root, "w").write("\n".join(b)+"\n")
    return syms

def build(components, out_path):
    syms = load_symbols(LIB)
    used = sorted({c["sym"] for c in components})
    root_uuid = uid()
    body = []
    body.append("(kicad_sch")
    body.append("\t(version 20260306)")
    body.append('\t(generator "eeschema")')
    body.append('\t(generator_version "10.0")')
    body.append(f'\t(uuid "{root_uuid}")')
    body.append('\t(paper "A2")')
    body.append(lib_symbols_section(syms, used))
    for c in components:
        s = syms[c["sym"]]
        value = c.get("value") or s["mpn"]
        fp = c.get("fp") or s["fp"]
        fields = dict(c.get("fields", {}))
        if s["lcsc"] and "LCSC" not in fields:
            fields["LCSC"] = s["lcsc"]
        body.append(comp_instance(c["ref"], c["sym"], c["at"], value,
                                  fp, root_uuid, fields))
        # labels
        pins = syms[c["sym"]]["pins"]
        nets = c["nets"]
        for num in nets:
            if num not in pins:
                print(f"WARN {c['ref']} pin {num} not in symbol {c['sym']}", file=sys.stderr)
        # every pin gets a label (its net) or a no-connect (unlisted / None)
        for num, pin in pins.items():
            net = nets.get(num)
            at = pin_abs(c["at"], pin)
            body.append(no_connect(at) if net is None else glabel(net, at))
    body.append('\t(sheet_instances (path "/" (page "1")))')
    body.append('\t(embedded_fonts no)')
    body.append(")")
    open(out_path,"w").write("\n".join(body)+"\n")
    return syms

if __name__ == "__main__":
    # PROBE: one AP22653, a global label on every pin, to validate format+geometry.
    comps = [{
        "ref":"U1", "sym":"AP22653W6-7", "at":(100,100,0),
        "value":"AP22653", "fp":"dongle-lite:SOT-23-6_L2.9-W1.6-P0.95-LS2.8-BR",
        "fields":{"LCSC":"C2158037"},
        "nets":{"1":"P5V_HOST","2":"GND","3":"TGT_VBUS_EN","4":"VBUS_FAULT","5":"ILIM_SET","6":"TGT_VBUS"},
    }]
    build(comps, "probe.kicad_sch")
    print("wrote probe.kicad_sch")
