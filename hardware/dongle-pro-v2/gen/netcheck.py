#!/usr/bin/env python3
"""Diff the exported KiCad netlist against the pin->net intent in board.py.
ERC cannot see a symbol pin wired to the wrong net, so this is the real check.
Run: kicad-cli sch export netlist --format kicadsexpr dongle-pro-v2.kicad_sch -o /tmp/v2.net
     python3 gen/netcheck.py /tmp/v2.net"""
import re, sys, os, runpy, io, contextlib
here = os.path.dirname(os.path.abspath(__file__))
os.chdir(os.path.join(here, ".."))
with contextlib.redirect_stdout(io.StringIO()):
    comps = runpy.run_path(os.path.join(here, "board.py"))["comps"]
want = {}
for cdef in comps:
    if cdef["ref"].startswith("#"): continue
    for pin, net in cdef["nets"].items():
        want[(cdef["ref"], pin)] = net
txt = open(sys.argv[1]).read()
got = {}
for blk in re.split(r'\(net\s+\(code', txt)[1:]:
    name = re.search(r'\(name "([^"]*)"\)', blk).group(1).lstrip("/")
    for ref, pin in re.findall(r'\(node\s+\(ref "([^"]+)"\)\s+\(pin "([^"]+)"\)', blk):
        got[(ref, pin)] = name
# auto-wired local nets come back as "Net-(...)": accept them when their node
# set is exactly the node set board.py gave one name
bynet = {}
for k, n in got.items(): bynet.setdefault(n, set()).add(k)
wantnet = {}
for k, n in want.items():
    if n: wantnet.setdefault(n, set()).add(k)
for n, nodes in bynet.items():
    if n.startswith("Net-("):
        names = {want.get(k) for k in nodes}
        if len(names) == 1 and None not in names and wantnet[next(iter(names))] == nodes:
            for k in nodes: got[k] = next(iter(names))
bad = 0
for k, net in sorted(want.items()):
    g = got.get(k)
    if net is None:
        if g and not g.startswith("unconnected-"):
            print("NC pin is connected:", k, g); bad += 1
    elif g != net:
        print("MISMATCH", k, "want", net, "got", g); bad += 1
for k, g in sorted(got.items()):
    if k not in want and not k[0].startswith("#"):
        print("EXTRA node", k, g); bad += 1
nets = {}
for (ref, pin), n in got.items():
    if not n.startswith("unconnected-"): nets.setdefault(n, []).append(f"{ref}.{pin}")
for n, nodes in sorted(nets.items()):
    if len(nodes) < 2: print("SINGLE-NODE net", n, nodes); bad += 1
print(f"{len(want)} pins checked, {len(nets)} nets, {bad} problems")
sys.exit(1 if bad else 0)
