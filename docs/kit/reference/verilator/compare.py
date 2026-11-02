#!/usr/bin/env python3
# EXAMPLE from V-Bear (Funtech Super A'Can, MiSTer core simulated with Verilator). It is a
# starting point: names, paths and the system's own details (BIOS loading, cartridge, video
# size, RAM port) must be adapted. See docs/kit-docs.md, "Validation with a reference".

# Compares V-Bear with the MiSTer core simulated in this folder: both run the same game, the
# picture and the 64 KB of work RAM are compared at each frame (0 = identical).
#
# Usage: reference/compare.sh [--jobs N] [--bios DIR] [--offset K] [--window W] [--cache DIR] V-BEAR ROM...
#   V-BEAR   V-Bear's binary (e.g. target/release/v-bear)
#   ROM      .bin, or .zip with one .bin inside
#   FRAMES   (environment) frames to compare, default "300"; one run of the reference per game and
#            frame (about 1 s per frame)
# The two boot at slightly different speeds (the reference reads the cartridge with a fixed delay,
# V-Bear does not), so reference frame N is compared with the V-Bear frames N-K-W .. N-K+W
# (K = --offset, default 1; W = --window, default 4) and the best one is reported (column LAG =
# N minus that frame). Frames with the fewest differing pixels win; among equals, the fewest RAM bytes.
# --cache DIR keeps the reference's picture and RAM, so a new run only repeats V-Bear's side.
import os, shutil, subprocess, sys, tempfile, zipfile
from concurrent.futures import ThreadPoolExecutor

here = os.path.dirname(os.path.abspath(__file__))
ref = os.path.join(here, "acan-ref")

def read_ppm(path):
    with open(path, "rb") as f:
        data = f.read()
    parts = data.split(b"\n", 3)
    w, h = map(int, parts[1].split())
    return w, h, parts[3]

def pixels(ref_ppm, vb_ppm):
    # V-Bear is always 320x240: H256 lines are stretched and the 224-line mode is centred
    # (crates/v-bear-core/src/video.rs); the reference has the real size.
    w, h, r = read_ppm(ref_ppm)
    vw, vh, v = read_ppm(vb_ppm)
    off = (vh - h) // 2
    diff = 0
    for y in range(h):
        for x in range(vw):
            a = (y + off) * vw * 3 + x * 3
            b = (y * w + x * w // vw) * 3
            if v[a:a + 3] != r[b:b + 3]:
                diff += 1
    return diff

def one(args):
    emu, bios, offset, window, cache, rom, n = args
    tmp = tempfile.mkdtemp()
    if rom.lower().endswith(".zip"):
        # a game on two mask ROMs (Super Dragon Force) comes as two files: joined, the largest first,
        # as the frontend does and as the MiSTer core expects them
        with zipfile.ZipFile(rom) as z:
            parts = sorted((z.read(m) for m in z.namelist() if not m.endswith("/")), key=len, reverse=True)
        game = os.path.join(tmp, os.path.splitext(os.path.basename(rom))[0] + ".bin")
        with open(game, "wb") as f:
            f.write(b"".join(parts))
    else:
        game = rom
    key = os.path.join(cache, "%s_%d" % (os.path.splitext(os.path.basename(rom))[0], n)) if cache else None
    if key and os.path.exists(key + ".ppm") and os.path.exists(key + ".ram"):
        shutil.copy(key + ".ppm", tmp + "/r.ppm"); shutil.copy(key + ".ram", tmp + "/r.ram")
    else:
        subprocess.run([ref, game, "--bios", bios, "--shot", str(n), tmp + "/r.ppm", "--dump-ram", tmp + "/r.ram"],
                       check=True, stderr=subprocess.DEVNULL)
        if key:
            os.makedirs(cache, exist_ok=True)
            shutil.copy(tmp + "/r.ppm", key + ".ppm"); shutil.copy(tmp + "/r.ram", key + ".ram")
    # every V-Bear frame around the target, to find the one that matches the reference
    os.makedirs(tmp + "/v")
    env = dict(os.environ, V_BEAR_SHOT=tmp + "/v/f", V_BEAR_SHOT_EVERY="1")
    subprocess.run([emu, rom, "--bios", bios, "--frames", str(n - offset + window)], check=True, env=env,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    cands = []
    for m in range(max(1, n - offset - window), n - offset + window + 1):
        cands.append((pixels(tmp + "/r.ppm", "%s/v/f-%05d.ppm" % (tmp, m)), m))
    low = min(d for d, _ in cands)
    # On a still picture many frames give the same pixel count; the picture cannot tell which of them
    # is the right alignment, so among them take the one whose work RAM differs least from the reference.
    ram_r = open(tmp + "/r.ram", "rb").read()
    best = None
    for d, m in cands:
        if d != low:
            continue
        env = dict(os.environ, V_BEAR_RAM=tmp + "/e%d.ram" % m)
        subprocess.run([emu, rom, "--bios", bios, "--frames", str(m)], check=True, env=env,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        r = sum(1 for a, b in zip(ram_r, open(tmp + "/e%d.ram" % m, "rb").read()) if a != b)
        if best is None or (r, abs(m - (n - offset))) < (best[0], abs(best[1] - (n - offset))):
            best = (r, m)
    ram, m = best
    px = low
    return os.path.basename(rom)[:40], n, n - m, px, ram


def main():
    a = sys.argv[1:]
    jobs, bios, offset, window, cache = 4, "bios", 1, 4, None
    while a and a[0].startswith("--"):
        k = a.pop(0)
        if k == "--jobs": jobs = int(a.pop(0))
        elif k == "--bios": bios = a.pop(0)
        elif k == "--offset": offset = int(a.pop(0))
        elif k == "--window": window = int(a.pop(0))
        elif k == "--cache": cache = os.path.abspath(a.pop(0))
    if len(a) < 2:
        sys.exit(__doc__)
    if not os.access(ref, os.X_OK):
        sys.exit("build the reference first: make -C " + here)
    emu, roms = a[0], a[1:]
    frames = [int(x) for x in os.environ.get("FRAMES", "300").split()]
    work = [(emu, os.path.abspath(bios), offset, window, cache, os.path.abspath(r), n) for r in roms for n in frames]
    print("%-40s %6s %4s %8s %8s" % ("GAME", "FRAME", "LAG", "PIXELS", "RAM"), flush=True)
    with ThreadPoolExecutor(jobs) as ex:
        for game, n, lag, px, ram in ex.map(one, work):
            print("%-40s %6d %4d %8d %8d" % (game, n, lag, px, ram), flush=True)

main()
