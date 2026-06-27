#!/usr/bin/env python3
"""Convert a Lucide SVG into an Android vector drawable (stroke-faithful).

Lucide icons are 24x24, stroke-based (stroke-width 2, round caps/joins, no fill),
built from <path>, <circle>, <line>, <rect>, <polyline>, <polygon>, <ellipse>.
Android vector drawables only understand <path>, so every primitive is converted
to path data and emitted with stroke attributes so the icon renders as the same
outline. strokeColor is a concrete black so Compose's `Icon(tint=…)` can recolor
it (tint multiplies onto non-transparent pixels). Output viewport is 24x24 to
match the source; the rendered size is controlled by the caller.

Usage: lucide_to_vd.py <in.svg> <out.xml>
"""
import re
import sys
import xml.etree.ElementTree as ET

SVG_NS = "{http://www.w3.org/2000/svg}"


def _f(v: str) -> float:
    return float(v)


def circle_to_path(cx, cy, r) -> str:
    cx, cy, r = _f(cx), _f(cy), _f(r)
    # Two arcs make a full circle (open, stroked — no fill).
    return (f"M{cx - r},{cy} "
            f"a{r},{r} 0 1,0 {2 * r},0 "
            f"a{r},{r} 0 1,0 {-2 * r},0 Z")


def ellipse_to_path(cx, cy, rx, ry) -> str:
    cx, cy, rx, ry = _f(cx), _f(cy), _f(rx), _f(ry)
    return (f"M{cx - rx},{cy} "
            f"a{rx},{ry} 0 1,0 {2 * rx},0 "
            f"a{rx},{ry} 0 1,0 {-2 * rx},0 Z")


def line_to_path(x1, y1, x2, y2) -> str:
    return f"M{_f(x1)},{_f(y1)} L{_f(x2)},{_f(y2)}"


def rect_to_path(x, y, w, h, rx=None, ry=None) -> str:
    x, y, w, h = _f(x), _f(y), _f(w), _f(h)
    if rx or ry:
        r = _f(rx if rx else ry)
        return (f"M{x + r},{y} h{w - 2 * r} a{r},{r} 0 0 1 {r},{r} "
                f"v{h - 2 * r} a{r},{r} 0 0 1 {-r},{r} h{-(w - 2 * r)} "
                f"a{r},{r} 0 0 1 {-r},{-r} v{-(h - 2 * r)} a{r},{r} 0 0 1 {r},{-r} Z")
    return f"M{x},{y} h{w} v{h} h{-w} Z"


def points_to_path(points: str, close: bool) -> str:
    nums = re.findall(r"-?\d*\.?\d+", points)
    pts = list(zip(nums[0::2], nums[1::2]))
    if not pts:
        return ""
    d = f"M{_f(pts[0][0])},{_f(pts[0][1])} " + " ".join(
        f"L{_f(x)},{_f(y)}" for x, y in pts[1:])
    return d + (" Z" if close else "")


def tag(el) -> str:
    return el.tag.replace(SVG_NS, "")


def to_path(el):
    t = tag(el)
    a = el.attrib
    if t == "path":
        return a.get("d")
    if t == "circle":
        return circle_to_path(a["cx"], a["cy"], a["r"])
    if t == "ellipse":
        return ellipse_to_path(a["cx"], a["cy"], a["rx"], a["ry"])
    if t == "line":
        return line_to_path(a["x1"], a["y1"], a["x2"], a["y2"])
    if t == "rect":
        return rect_to_path(a["x"], a["y"], a["width"], a["height"],
                            a.get("rx"), a.get("ry"))
    if t == "polyline":
        return points_to_path(a["points"], close=False)
    if t == "polygon":
        return points_to_path(a["points"], close=True)
    return None


def main():
    src, dst = sys.argv[1], sys.argv[2]
    tree = ET.parse(src)
    root = tree.getroot()
    sw = root.attrib.get("stroke-width", "2")
    cap = root.attrib.get("stroke-linecap", "round")
    join = root.attrib.get("stroke-linejoin", "round")

    paths = []
    for el in root.iter():
        if el is root:
            continue
        d = to_path(el)
        if d:
            paths.append(d.strip())

    lines = ['<vector xmlns:android="http://schemas.android.com/apk/res/android"',
             '    android:width="24dp"',
             '    android:height="24dp"',
             '    android:viewportWidth="24"',
             '    android:viewportHeight="24">']
    for d in paths:
        lines += [
            '    <path',
            f'        android:pathData="{d}"',
            '        android:fillColor="#00000000"',
            '        android:strokeColor="#FF000000"',
            f'        android:strokeWidth="{sw}"',
            f'        android:strokeLineCap="{cap}"',
            f'        android:strokeLineJoin="{join}"/>']
    lines.append('</vector>')
    with open(dst, "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
