"""Routing helpers for gen/route.py: filleted tracks, matched differential
pairs with concentric arcs, vias. KiCad python (pcbnew)."""
import math, pcbnew

class Router:
    def __init__(self, path):
        self.board = pcbnew.LoadBoard(path)
        b = self.board
        # resolve every lookup before the first edit (swig goes stale after)
        self.nets = {str(k): v for k, v in b.GetNetsByName().items()}
        self.pads = {}
        for fp in b.GetFootprints():
            for pad in fp.Pads():
                p = pad.GetPosition()
                self.pads.setdefault((fp.GetReference(), pad.GetNumber()), []).append(
                    ((p.x / 1e6, p.y / 1e6), pad.GetNetname()))
        self.L = {'F': pcbnew.F_Cu, 'B': pcbnew.B_Cu, 'I1': pcbnew.In1_Cu, 'I2': pcbnew.In2_Cu}
        for t in list(b.GetTracks()):
            b.Remove(t)
        self.count = 0

    def P(self, spec):
        """'U3.1' -> (x, y) of that pad (first one if the number repeats)."""
        ref, num = spec.split('.', 1)
        return self.pads[(ref, num)][0][0]

    def netof(self, spec):
        ref, num = spec.split('.', 1)
        return self.pads[(ref, num)][0][1]

    def _pt(self, p):
        return pcbnew.VECTOR2I(pcbnew.FromMM(p[0]), pcbnew.FromMM(p[1]))

    def _resolve(self, pts):
        return [self.P(p) if isinstance(p, str) else (float(p[0]), float(p[1])) for p in pts]

    @staticmethod
    def fillet(pts, r):
        """Polyline -> ('line', a, b) / ('arc', start, mid, end) with every bend
        rounded to radius r (shrunk where a leg is too short)."""
        pts = [p for i, p in enumerate(pts) if i == 0 or math.dist(p, pts[i - 1]) > 1e-6]
        out, cur = [], pts[0]
        for a, b, c in zip(pts, pts[1:], pts[2:]):
            ux, uy = b[0] - a[0], b[1] - a[1]; vx, vy = c[0] - b[0], c[1] - b[1]
            lu, lv = math.hypot(ux, uy), math.hypot(vx, vy)
            ux, uy, vx, vy = ux / lu, uy / lu, vx / lv, vy / lv
            turn = math.acos(max(-1.0, min(1.0, ux * vx + uy * vy)))
            if turn < math.radians(3) or r <= 0:
                out.append(('line', cur, b)); cur = b; continue
            d = min(r * math.tan(turn / 2), lu * 0.49, lv * 0.49)
            rr = d / math.tan(turn / 2)
            p1 = (b[0] - ux * d, b[1] - uy * d); p2 = (b[0] + vx * d, b[1] + vy * d)
            wx, wy = vx - ux, vy - uy; wl = math.hypot(wx, wy); wx, wy = wx / wl, wy / wl
            k = d / math.sin(turn / 2) - rr
            out.append(('line', cur, p1)); out.append(('arc', p1, (b[0] + wx * k, b[1] + wy * k), p2)); cur = p2
        out.append(('line', cur, pts[-1]))
        return out

    def track(self, net, layer, pts, w=0.15, r=0.5):
        pts = self._resolve(pts)
        for piece in self.fillet(pts, r):
            if piece[0] == 'line':
                if math.dist(piece[1], piece[2]) < 1e-4: continue
                t = pcbnew.PCB_TRACK(self.board)
                t.SetStart(self._pt(piece[1])); t.SetEnd(self._pt(piece[2]))
            else:
                t = pcbnew.PCB_ARC(self.board)
                t.SetStart(self._pt(piece[1])); t.SetMid(self._pt(piece[2])); t.SetEnd(self._pt(piece[3]))
            t.SetWidth(pcbnew.FromMM(w)); t.SetLayer(self.L[layer]); t.SetNet(self.nets[net])
            self.board.Add(t); self.count += 1

    def via(self, net, xy, d=0.45, drill=0.2):
        xy = self._resolve([xy])[0]
        v = pcbnew.PCB_VIA(self.board)
        v.SetPosition(self._pt(xy)); v.SetWidth(pcbnew.F_Cu, pcbnew.FromMM(d)); v.SetDrill(pcbnew.FromMM(drill))
        v.SetViaType(pcbnew.VIATYPE_THROUGH); v.SetNet(self.nets[net]); self.board.Add(v)
        return xy

    @staticmethod
    def offset(pts, off):
        """Mitred parallel copy of a polyline, `off` mm to the left of travel
        (negative = right). Ends are offset square to the first/last leg."""
        n = len(pts); out = []
        def nrm(a, b):
            dx, dy = b[0] - a[0], b[1] - a[1]; l = math.hypot(dx, dy)
            return (dy / l, -dx / l)          # left of travel with y down
        for i, p in enumerate(pts):
            if i == 0: nx, ny = nrm(pts[0], pts[1]); s = 1.0
            elif i == n - 1: nx, ny = nrm(pts[-2], pts[-1]); s = 1.0
            else:
                n1 = nrm(pts[i - 1], p); n2 = nrm(p, pts[i + 1])
                nx, ny = n1[0] + n2[0], n1[1] + n2[1]; l = math.hypot(nx, ny); nx, ny = nx / l, ny / l
                s = 1.0 / max(0.2, nx * n1[0] + ny * n1[1])
            out.append((p[0] + nx * off * s, p[1] + ny * off * s))
        return out

    def pair(self, net_l, net_r, layer, pts, w=0.211, gap=0.127, r=0.8, ends=((), ())):
        """Coupled pair along centreline `pts`. net_l runs on the left of travel,
        net_r on the right. Bends use concentric arcs so the gap is constant.
        `ends` = ((start_l, start_r), (end_l, end_r)): pad specs / points each
        leg is stubbed to at that end (omit to leave the leg open)."""
        pts = self._resolve(pts); s = (w + gap) / 2
        # which side is inside each bend decides the radius
        for net, off in ((net_l, +s), (net_r, -s)):
            line = self.offset(pts, off)
            pre = [ends[0][0 if off > 0 else 1]] if ends[0] else []
            post = [ends[1][0 if off > 0 else 1]] if ends[1] else []
            full = self._resolve(pre) + line + self._resolve(post)
            self._pair_leg(net, layer, full, pts, off, r, w, len(pre))
        return pts

    def _pair_leg(self, net, layer, full, centre, off, r, w, npre):
        # per-corner radius: centreline radius +/- offset depending on turn direction
        def turn_sign(a, b, c):
            return (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
        pieces = []; cur = full[0]
        for i in range(1, len(full) - 1):
            a, b, c = full[i - 1], full[i], full[i + 1]
            ci = i - npre
            rr = 0.25
            if 1 <= ci <= len(centre) - 2:
                sgn = turn_sign(centre[ci - 1], centre[ci], centre[ci + 1])
                # y-down: sgn > 0 is a right-hand (clockwise) turn; the left leg is outside
                rr = r + (abs(off) if (sgn > 0) == (off > 0) else -abs(off))
            seg = self.fillet([cur, b, c], max(rr, 0.12))
            pieces += seg[:-1]; cur = seg[-1][1]
        pieces.append(('line', cur, full[-1]))
        for piece in pieces:
            if piece[0] == 'line':
                if math.dist(piece[1], piece[2]) < 1e-4: continue
                t = pcbnew.PCB_TRACK(self.board); t.SetStart(self._pt(piece[1])); t.SetEnd(self._pt(piece[2]))
            else:
                t = pcbnew.PCB_ARC(self.board)
                t.SetStart(self._pt(piece[1])); t.SetMid(self._pt(piece[2])); t.SetEnd(self._pt(piece[3]))
            t.SetWidth(pcbnew.FromMM(w)); t.SetLayer(self.L[layer]); t.SetNet(self.nets[net])
            self.board.Add(t); self.count += 1

    def save(self, path):
        pcbnew.SaveBoard(path, self.board)
