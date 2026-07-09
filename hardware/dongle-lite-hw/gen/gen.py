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

def comp_instance(ref, name, at, value, footprint, root_uuid, fields, project="dongle-lite-hw"):
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

def glabel(net, at):
    X,Y = at
    return (f'\t(global_label "{net}" (shape bidirectional) (at {X} {Y} 0)\n'
            f'\t\t(effects (font (size 1.27 1.27)) (justify left))\n'
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
