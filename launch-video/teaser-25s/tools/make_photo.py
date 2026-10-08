"""Generate assets/scan.jpg: a phone photo of a signed paper page on a table.

Usage (from the teaser-25s folder):  python3 tools/make_photo.py
Needs numpy and Pillow, plus the Liberation Serif fonts. Fixed seed, so the
output is identical on every run. The whole image is synthetic: no stock
photo, no licence to track.
"""
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

W, H = 1600, 1200
rng = np.random.default_rng(4127)
SERIF = "/usr/share/fonts/truetype/liberation/LiberationSerif-Regular.ttf"
SERIF_B = "/usr/share/fonts/truetype/liberation/LiberationSerif-Bold.ttf"


def smooth_noise(h, w, scale_y, scale_x):
    """Low-frequency noise by upsampling a small random grid."""
    small = rng.standard_normal((max(2, h // scale_y), max(2, w // scale_x)))
    img = Image.fromarray(((small - small.min()) / np.ptp(small) * 255).astype(np.uint8))
    return np.asarray(img.resize((w, h), Image.BICUBIC), dtype=np.float64) / 255.0


# ---------------------------------------------------------------- table: warm oak, light from the top left
yy, xx = np.mgrid[0:H, 0:W].astype(np.float64)
grain = 0.55 * smooth_noise(H, W, 6, 220) + 0.3 * smooth_noise(H, W, 2, 60) + 0.15 * rng.random((H, W))
rings = 0.5 + 0.5 * np.sin(yy / 23.0 + 9 * smooth_noise(H, W, 40, 300))
wood = 0.62 * grain + 0.38 * rings
base = np.array([128, 86, 52], dtype=np.float64)
dark = np.array([84, 52, 30], dtype=np.float64)
table = dark + (base - dark) * wood[..., None]
light = 1.18 - 0.55 * np.hypot((xx - 300) / W, (yy - 150) / H)
table *= light[..., None]
img = Image.fromarray(np.clip(table, 0, 255).astype(np.uint8)).convert("RGBA")

# ---------------------------------------------------------------- the page (drawn flat, then put in perspective)
PW, PH = 1000, 1414
page = Image.new("RGBA", (PW, PH), (250, 248, 243, 255))
d = ImageDraw.Draw(page)
f_title = ImageFont.truetype(SERIF_B, 46)
f_sub = ImageFont.truetype(SERIF, 24)
f_head = ImageFont.truetype(SERIF_B, 25)
f_body = ImageFont.truetype(SERIF, 21)
ink = (38, 36, 34, 255)

def centred(y, text, font, fill=ink):
    w = d.textlength(text, font=font)
    d.text(((PW - w) / 2, y), text, font=font, fill=fill)

centred(96, "RESIDENTIAL LEASE AGREEMENT", f_title)
centred(162, "Unit 4B  ·  118 Alder Street", f_sub, (90, 88, 84, 255))
d.line([(110, 214), (PW - 110, 214)], fill=(150, 146, 140, 255), width=2)

clauses = [
    ("1. Parties", "This agreement is made between the Landlord and the Tenant named below for the premises described above."),
    ("2. Term", "The lease begins on the first day of the month following signature and continues for twelve (12) months."),
    ("3. Rent", "Rent is due on the first day of each month. A late fee applies to payments received after the fifth day."),
    ("4. Deposit", "A security deposit equal to one month of rent is held for the term and returned within thirty days of move-out."),
    ("5. Maintenance", "The Tenant keeps the premises clean and reports damage promptly. The Landlord handles structural repairs."),
    ("6. Entry", "The Landlord may enter with twenty-four hours notice, except in an emergency."),
]

def wrap(text, font, width):
    words, lines, cur = text.split(), [], ""
    for w in words:
        t = (cur + " " + w).strip()
        if d.textlength(t, font=font) > width:
            lines.append(cur); cur = w
        else:
            cur = t
    return lines + [cur]

y = 262
for head, body in clauses:
    d.text((110, y), head, font=f_head, fill=ink); y += 40
    for line in wrap(body, f_body, PW - 220):
        d.text((110, y), line, font=f_body, fill=(58, 56, 53, 255)); y += 31
    y += 22

# Signature block
sy = 1170
for x0, label in [(110, "Landlord"), (560, "Tenant")]:
    d.line([(x0, sy), (x0 + 330, sy)], fill=(70, 68, 64, 255), width=2)
    d.text((x0, sy + 12), label, font=f_sub, fill=(90, 88, 84, 255))

def scribble(points, width, colour):
    """Pen stroke through control points (Catmull-Rom), drawn as dense dots."""
    pts = np.array(points, dtype=np.float64)
    pts = np.vstack([pts[0], pts, pts[-1]])
    out = []
    for i in range(1, len(pts) - 2):
        p0, p1, p2, p3 = pts[i - 1:i + 3]
        steps = max(6, int(np.hypot(*(p2 - p1)) / 0.8))
        for t in np.linspace(0, 1, steps, endpoint=False):
            t2, t3 = t * t, t * t * t
            out.append(0.5 * ((2 * p1) + (-p0 + p2) * t + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t2 + (-p0 + 3 * p1 - 3 * p2 + p3) * t3))
    for i, (x, yv) in enumerate(out):
        r = width * (0.75 + 0.35 * np.sin(i / 9.0))
        d.ellipse([x - r, yv - r, x + r, yv + r], fill=colour)

blue = (30, 52, 128, 235)

def cursive(x0, y0, length, height, seed, turns=7):
    """Signature-like stroke: a slanted trochoid. Where the wobble radius beats the
    forward speed the pen loops back; elsewhere it only humps, like fast handwriting."""
    r = np.random.default_rng(seed)
    th = np.linspace(0, 2 * np.pi * turns, 600)
    knots = np.linspace(0, th[-1], turns + 2)
    v = length / th[-1]
    rad = np.interp(th, knots, v * r.uniform(0.5, 1.7, len(knots)))
    hgt = np.interp(th, knots, height * r.uniform(0.35, 0.9, len(knots)))
    hgt[th < 2 * np.pi] *= 1.7                                   # tall opening letter
    y = y0 - hgt * (1 - np.cos(th)) / 2
    x = x0 + v * th - rad * np.sin(th) + 0.38 * (y0 - y)        # forward slant
    scribble(list(zip(x[::3], y[::3])), 2.2, blue)

cursive(130, 1150, 220, 34, 3, 6)                             # Landlord
scribble([(124, 1162), (250, 1158), (372, 1148)], 1.6, blue)
cursive(590, 1152, 250, 40, 21, 8)                            # Tenant
scribble([(600, 1166), (740, 1162), (884, 1146)], 1.6, blue)

# Paper texture: fibre noise plus a soft fold across the middle.
p = np.asarray(page, dtype=np.float64)
fibre = (smooth_noise(PH, PW, 3, 3) - 0.5) * 6 + rng.standard_normal((PH, PW)) * 1.6
fold = 1 - 0.035 * np.exp(-((np.arange(PH)[:, None] - PH * 0.5) / 18.0) ** 2)
p[..., :3] = p[..., :3] * fold[..., None] + fibre[..., None]
page = Image.fromarray(np.clip(p, 0, 255).astype(np.uint8))

# ---------------------------------------------------------------- perspective: phone held slightly off-axis
def coeffs(dst, src):
    """PIL perspective coefficients that map output quad `dst` onto source quad `src`."""
    A, b = [], []
    for (x, yv), (u, v) in zip(dst, src):
        A.append([x, yv, 1, 0, 0, 0, -u * x, -u * yv]); b.append(u)
        A.append([0, 0, 0, x, yv, 1, -v * x, -v * yv]); b.append(v)
    return np.linalg.solve(np.array(A), np.array(b))

quad = [(468, 58), (1188, 92), (1232, 1150), (424, 1128)]   # tl, tr, br, bl on the photo
src = [(0, 0), (PW, 0), (PW, PH), (0, PH)]
warped = page.transform((W, H), Image.PERSPECTIVE, coeffs(quad, src), Image.BICUBIC)

# Contact shadow under the page.
shadow = Image.new("L", (W, H), 0)
ImageDraw.Draw(shadow).polygon([(x + 10, yv + 16) for x, yv in quad], fill=150)
shadow = shadow.filter(ImageFilter.GaussianBlur(22))
img = Image.composite(Image.new("RGBA", (W, H), (20, 12, 6, 255)), img, shadow.point(lambda v: int(v * 0.75)))
img.alpha_composite(warped)

# ---------------------------------------------------------------- pen resting across the corner
pen = Image.new("RGBA", (580, 52), (0, 0, 0, 0))
pd = ImageDraw.Draw(pen)
pd.rounded_rectangle([0, 12, 470, 40], radius=14, fill=(28, 30, 38, 255))
pd.polygon([(470, 14), (520, 24), (530, 26), (520, 28), (470, 38)], fill=(176, 174, 166, 255))
pd.rounded_rectangle([30, 17, 440, 22], radius=3, fill=(96, 100, 116, 255))      # highlight
pd.rounded_rectangle([40, 4, 190, 13], radius=4, fill=(156, 156, 160, 255))      # clip
pen = pen.rotate(152, expand=True, resample=Image.BICUBIC)   # tip toward the page, body off the corner
pen_sh = Image.new("RGBA", pen.size, (0, 0, 0, 0))
pen_sh.putalpha(pen.getchannel("A").point(lambda v: int(v * 0.55)))
pen_sh = pen_sh.filter(ImageFilter.GaussianBlur(9))
img.alpha_composite(pen_sh, (1130 + 14, 860 + 22))
img.alpha_composite(pen, (1130, 860))

# ---------------------------------------------------------------- camera: warm light falloff, vignette, sensor noise
a = np.asarray(img.convert("RGB"), dtype=np.float64)
fall = 1.06 - 0.22 * np.hypot((xx - 700) / W, (yy - 420) / H) ** 1.2
vign = 1 - 0.38 * (np.hypot((xx - W / 2) / (W / 2), (yy - H / 2) / (H / 2)) / 1.414) ** 2.2
a *= (fall * vign)[..., None]
a *= np.array([1.03, 1.0, 0.95])                     # indoor warmth
a += rng.standard_normal(a.shape) * 2.2
out = Image.fromarray(np.clip(a, 0, 255).astype(np.uint8)).filter(ImageFilter.GaussianBlur(0.6))
out.save("assets/scan.jpg", quality=90, optimize=True)
print("wrote assets/scan.jpg", out.size)
