#!/usr/bin/env python3
"""Generate or import designer assets, validate formats, and build a review sheet.

Built-in image_gen: --plan, then --import-image ID=PATH --model image_gen.
API: wraps the imagegen skill CLI; choose --model explicitly and set OPENAI_API_KEY.
Requires Pillow for postprocessing. --only ID intentionally replaces that asset.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
TRIAL = ("isr_vautour", "jor_m48", "briefing_sixday67")
REPORT = ROOT / "assets/generation-report.json"
PATTERN = re.compile(r"### (\w+)\nСохранить: `([^`]+)`[^\n]*\n\n```\n(.*?)\n```", re.S)


def jobs(spec: Path) -> list[dict]:
    result = []
    for asset_id, filename, prompt in PATTERN.findall(spec.read_text(encoding="utf-8")):
        path = ROOT / filename
        if not path.resolve().is_relative_to((ROOT / "assets").resolve()):
            raise ValueError(f"Asset outside assets/: {filename}")
        mini = "units_minis" in path.parts
        illustration = path.parent.name in ("briefing", "events")
        kind = "miniature" if mini else "illustration" if illustration else "object" if path.parent.name == "objects" else "ui" if path.parent.name == "ui" else "terrain"
        result.append(dict(id=asset_id, source=filename,
                           file=str(path.with_suffix(".webp").relative_to(ROOT)) if illustration or kind == "ui" else filename,
                           prompt=prompt, prompt_source=str(spec.resolve().relative_to(ROOT)),
                           prompt_version="midlands-v1" if spec.name == "assets-midlands.md" else "v1", size="1536x1024" if illustration or kind == "ui" else "1024x1024",
                           background="transparent" if mini or kind == "object" else "opaque",
                           kind=kind))
    if not result or len({j["id"] for j in result}) != len(result):
        raise ValueError("Spec has no assets or duplicate IDs")
    return result


def read_json(path: Path, default):
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else default


def write_json(path: Path, obj):
    path.parent.mkdir(parents=True, exist_ok=True)
    # Atomic replacement keeps manifests usable if a run is interrupted.
    pending = path.with_suffix(path.suffix + ".tmp")
    pending.write_text(json.dumps(obj, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    pending.replace(path)


def terrain_seams(image):
    """Same opposite-edge/inner-neighbour metric as sim assets-check."""
    from PIL import ImageChops, ImageStat
    rgb = image.convert("RGB")
    w, h = rgb.size
    def mean(a, b):
        return sum(ImageStat.Stat(ImageChops.difference(a, b)).mean) / 3
    lr = mean(rgb.crop((0,0,1,h)), rgb.crop((w-1,0,w,h)))
    tb = mean(rgb.crop((0,0,w,1)), rgb.crop((0,h-1,w,h)))
    ix = mean(rgb.crop((0,0,w-1,h)), rgb.crop((1,0,w,h)))
    iy = mean(rgb.crop((0,0,w,h-1)), rgb.crop((0,1,w,h)))
    def ratio(edge, inner):
        return edge / inner if inner else float("inf") if edge else 1.0
    return ratio(lr, ix), ratio(tb, iy)


def wrap_terrain_edges(image, padding=32):
    """Blend only narrow opposing margins into a continuous periodic tile."""
    result = image.copy()
    pixels = result.load()
    w, h = result.size
    for x in range(padding):
        weight = 0.5 * (1 - x / padding) ** 2
        for y in range(h):
            a, b = pixels[x, y], pixels[w-1-x, y]
            pixels[x, y] = tuple(round(v*(1-weight)+z*weight) for v,z in zip(a,b))
            pixels[w-1-x, y] = tuple(round(z*(1-weight)+v*weight) for v,z in zip(a,b))
    for y in range(padding):
        weight = 0.5 * (1 - y / padding) ** 2
        for x in range(w):
            a, b = pixels[x, y], pixels[x, h-1-y]
            pixels[x, y] = tuple(round(v*(1-weight)+z*weight) for v,z in zip(a,b))
            pixels[x, h-1-y] = tuple(round(z*(1-weight)+v*weight) for v,z in zip(a,b))
    return result


def finish(source: Path, job: dict, output: Path) -> tuple[int, int]:
    from PIL import Image, ImageOps
    with Image.open(source) as raw:
        raw.load()  # Reject truncated or unreadable responses before touching the destination.
        if job["kind"] == "miniature":
            if raw.width != raw.height or min(raw.size) < 512:
                raise ValueError("Miniature must be square and at least 512 px")
            if "A" not in raw.getbands() or raw.getchannel("A").getextrema()[0] != 0:
                raise ValueError("Miniature has no genuine transparency")
            # Fit opaque bounds inside a 75% frame and place contact at y=0.88.
            # This preserves alpha and compensates for generated framing drift.
            rgba = raw.convert("RGBA")
            # Ignore isolated almost-transparent generator noise when framing.
            bounds = rgba.getchannel("A").point(lambda a: 255 if a >= 8 else 0).getbbox()
            if bounds is None:
                raise ValueError("Empty miniature")
            rgba = rgba.crop(bounds)
            solid = rgba.getchannel("A").point(lambda a: 255 if a >= 128 else 0).getbbox()
            if solid is None:
                raise ValueError("Empty miniature")
            x0, y0, x1, y1 = solid
            scale = min(0.75 * 1024 / (x1 - x0), 0.75 * 1024 / (y1 - y0))
            # Never let translucent shadows or antialiasing touch the outer border.
            scale = min(scale, 1020 / rgba.width, 1020 / rgba.height)
            scaled = rgba.resize((round(rgba.width * scale), round(rgba.height * scale)), Image.Resampling.LANCZOS)
            x = round(512 - (x0 + x1) * scale / 2)
            y = round(0.88 * 1024 - y1 * scale)
            if x < 1 or y < 1 or x + scaled.width > 1023 or y + scaled.height > 1023:
                # Keep the full source alpha (no cropping) while retaining the contact anchor.
                scale = min(scale, 0.87 * 1024 / y1, 0.11 * 1024 / max(1, rgba.height - y1),
                            0.49 * 1024 / max((x0 + x1) / 2, rgba.width - (x0 + x1) / 2))
                scaled = rgba.resize((round(rgba.width * scale), round(rgba.height * scale)), Image.Resampling.LANCZOS)
                x = round(512 - (x0 + x1) * scale / 2)
                y = round(0.88 * 1024 - y1 * scale)
            canvas = Image.new("RGBA", (1024, 1024))
            canvas.alpha_composite(scaled, (x, y))
            image = canvas.resize((512, 512), Image.Resampling.LANCZOS)
            # Alpha ringing can add a subpixel trace; require a clean outer pixel.
            alpha = image.getchannel("A")
            if any(alpha.getpixel(p) for i in range(512) for p in ((i, 0), (i, 511), (0, i), (511, i))):
                raise ValueError("Miniature touches the frame")
        elif job["kind"] == "object":
            if raw.width != raw.height or min(raw.size) < 512:
                raise ValueError("Object must be square and at least 512 px")
            if "A" not in raw.getbands() or raw.getchannel("A").getextrema()[0] != 0:
                raise ValueError("Object has no genuine transparency")
            image = Image.new("RGBA", (512, 512))
            image.alpha_composite(raw.convert("RGBA").resize((480, 480), Image.Resampling.LANCZOS), (16, 16))
            bounds = image.getchannel("A").getbbox()
            if bounds is None or min(bounds[0], bounds[1], 512-bounds[2], 512-bounds[3]) < 8:
                raise ValueError("Object needs at least 8 px transparent padding")
        elif job["kind"] == "ui":
            image = ImageOps.fit(raw.convert("RGB"), (1920, 1080), Image.Resampling.LANCZOS)
        elif job["kind"] == "illustration":
            size = (1600, 900) if output.parent.name == "briefing" else (1200, 675)
            image = ImageOps.fit(raw.convert("RGB"), size, Image.Resampling.LANCZOS, centering=(0.5, 0.5))
        else:
            if raw.width != raw.height or raw.width < 1024:
                raise ValueError("Terrain must be square and at least 1024 px")
            image = raw.convert("RGB").resize((1024, 1024), Image.Resampling.LANCZOS)
            ratios = terrain_seams(image)
            if job.get("wrap_edges") and max(ratios) > 1.5:
                image = wrap_terrain_edges(image)
                ratios = terrain_seams(image)
            if max(ratios) > 1.5:
                raise ValueError(f"Terrain seam ratios {ratios[0]:.2f}, {ratios[1]:.2f} exceed 1.5")
        output.parent.mkdir(parents=True, exist_ok=True)
        pending = output.with_name(output.stem + ".pending" + output.suffix)
        if output.suffix == ".webp":
            image.save(pending, format="WEBP", quality=85, method=6)
        else:
            image.save(pending, format="PNG", optimize=True)
        with Image.open(pending) as saved:
            saved.verify()
        pending.replace(output)
        return image.size


def manifest(job: dict, model: str, size: tuple[int, int]):
    mini = job["kind"] == "miniature"
    base = ROOT / "assets/units_minis" if mini else ROOT / "assets"
    path = base / "manifest.json"
    entries = read_json(path, [])
    entry = next((e for e in entries if e["id"] == job["id"]), None)
    if entry is None:
        entry = dict(id=job["id"])
        entries.append(entry)
    file = ROOT / job["file"]
    entry.update(file=str(file.relative_to(base)), width=size[0], height=size[1], prompt=job["prompt"],
                 source=job["prompt_source"], prompt_version=job["prompt_version"],
                 model=model, generation_backend="image_gen" if model == "image_gen" else "openai-images-api")
    if job["kind"] == "object":
        entry["postprocess"] = "Fit full source alpha into 480 px with 16 px transparent padding"
    if job.get("wrap_edges"):
        entry["postprocess"] = "32 px opposing-edge blend if needed, then seam validation"
    if model == "image_gen":
        entry["model_note"] = "Built-in image_gen does not expose the underlying model ID"
    if mini:
        entry.update(side=file.parent.name, template="air" if "In flight" in job["prompt"] else "ground",
                     style="miniature", anchor=[0.5, 0.88])
    else:
        group = {"terrain": "terrain", "object": "object", "ui": "ui"}.get(job["kind"], "briefing" if file.parent.name == "briefing" else "event")
        entry.update(side=None, group=group,
                     template=job["kind"] if job["kind"] in ("terrain", "object", "ui") else "illustration")
    write_json(path, entries)


def sheet():
    from PIL import Image, ImageDraw, ImageOps
    # Existing miniatures and illustration are shown beside the new trial assets.
    refs = [("Existing: Centurion", ROOT / "assets/units_minis/isr/isr_centurion.png"),
            ("New: Jordan M48", ROOT / "assets/units_minis/jor/jor_m48.png"),
            ("Existing: Mirage III", ROOT / "assets/units_minis/isr/isr_mirage3.png"),
            ("New: Vautour", ROOT / "assets/units_minis/isr/isr_vautour.png"),
            ("Existing: Sinai briefing", ROOT / "assets/briefing/briefing_sinai67.webp"),
            ("New: Six-Day War briefing", ROOT / "assets/briefing/briefing_sixday67.webp")]
    board = Image.new("RGB", (1200, 1040), "#eee9de")
    draw = ImageDraw.Draw(board)
    for i, (label, path) in enumerate(refs):
        x, y = (i % 2) * 600, (i // 2) * 345
        draw.text((x + 16, y + 8), label, fill="#282620")
        if not path.exists():
            draw.text((x + 16, y + 80), "Pending", fill="#735b4b")
            continue
        with Image.open(path) as raw:
            image = ImageOps.contain(raw.convert("RGBA"), (568, 300), Image.Resampling.LANCZOS)
            tile = Image.new("RGBA", (568, 300), "#d2c9b4")
            if raw.mode == "RGBA":
                td = ImageDraw.Draw(tile)
                for sy in range(0, 300, 16):
                    for sx in range(0, 568, 16):
                        if (sx // 16 + sy // 16) % 2:
                            td.rectangle((sx, sy, sx + 15, sy + 15), fill="#e4ddce")
            tile.alpha_composite(image, ((568 - image.width) // 2, (300 - image.height) // 2))
            board.paste(tile.convert("RGB"), (x + 16, y + 32))
    path = ROOT / "assets/units_minis/contact_sheet_trial.png"
    board.save(path)
    return path


def full_sheets(all_jobs: list[dict]):
    from PIL import Image, ImageDraw, ImageOps
    for kind, width, height, cols, destination in [
        ("miniature", 256, 280, 6, "assets/units_minis/contact_sheet_all.png"),
        ("illustration", 480, 300, 3, "assets/contact_sheet_illustrations.png"),
        ("terrain", 512, 548, 2, "assets/contact_sheet_terrain.png"),
    ]:
        selected = [j for j in all_jobs if j["kind"] == kind]
        if not selected:
            continue
        rows = (len(selected) + cols - 1) // cols
        board = Image.new("RGB", (width * cols, height * rows), "#e8e1d2")
        draw = ImageDraw.Draw(board)
        for i, job in enumerate(selected):
            x, y = (i % cols) * width, (i // cols) * height
            draw.text((x + 8, y + 8), job["id"], fill="#282620")
            path = ROOT / job["file"]
            if not path.exists():
                draw.text((x + 8, y + 40), "MISSING", fill="red")
                continue
            with Image.open(path) as source:
                if kind == "terrain":
                    # Four repeats expose both seam directions at the centre.
                    tile = source.convert("RGB").resize((width // 2, width // 2), Image.Resampling.LANCZOS)
                    for dx, dy in [(0,0), (width//2,0), (0,width//2), (width//2,width//2)]:
                        board.paste(tile, (x+dx, y+28+dy))
                else:
                    image = ImageOps.contain(source.convert("RGBA"), (width-16, height-36), Image.Resampling.LANCZOS)
                    back = Image.new("RGBA", image.size, "#d2c9b4")
                    back.alpha_composite(image)
                    board.paste(back.convert("RGB"), (x+(width-image.width)//2, y+28))
        path = ROOT / destination
        path.parent.mkdir(parents=True, exist_ok=True)
        board.save(path)
        print(f"Contact sheet: {path}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec", type=Path, default=ROOT / "assets-to-generate.md")
    parser.add_argument("--only", action="append", help="Regenerate one ID; repeat to select more")
    parser.add_argument("--trial", action="store_true", help="Select the three approval samples")
    parser.add_argument("--sheet-all", action="store_true", help="Build complete miniature, illustration and terrain review sheets")
    parser.add_argument("--plan", action="store_true", help="Print built-in image_gen jobs; no generation")
    parser.add_argument("--wrap-edges", action="store_true", help="Blend terrain margins before seam validation")
    parser.add_argument("--model", help="Explicit API model, or image_gen when importing built-in results")
    parser.add_argument("--import-image", action="append", default=[], metavar="ID=PATH")
    parser.add_argument("--reject", nargs=2, metavar=("ID", "REASON"), help="Record a visual rejection; then regenerate with --only")
    parser.add_argument("--attempts", type=int, choices=(1, 2, 3), default=3)
    parser.add_argument("--cli", type=Path, default=Path(os.environ.get("CODEX_HOME", str(Path.home() / ".codex"))) / "skills/.system/imagegen/scripts/image_gen.py")
    args = parser.parse_args()
    all_jobs = jobs(args.spec)
    for job in all_jobs:
        job["wrap_edges"] = args.wrap_edges and job["kind"] == "terrain"
    known = {j["id"] for j in all_jobs}
    imports = dict(pair.split("=", 1) for pair in args.import_image)
    selected = set(args.only or (TRIAL if args.trial else imports or known))
    if not selected <= known or not imports.keys() <= known:
        parser.error("Unknown asset ID")
    report = read_json(REPORT, {"assets": {}, "rejected": []})
    if args.reject:
        asset_id, reason = args.reject
        if asset_id not in known:
            parser.error("Unknown rejection ID")
        report["rejected"].append(dict(id=asset_id, reason=reason, kind="visual", attempt=report["assets"].get(asset_id, {}).get("attempts", 1)))
        report["assets"].setdefault(asset_id, {"attempts": 1})["status"] = "rejected"
        write_json(REPORT, report)
        return 0
    plan = [j for j in all_jobs if j["id"] in selected and (args.only or j["id"] in imports or not (ROOT / j["file"]).exists())]
    if args.plan:
        print(json.dumps(plan, ensure_ascii=False, indent=2))
        return 0
    if plan and not args.model:
        parser.error("Choose --model for API generation; for built-in results use --import-image ID=PATH --model image_gen. The built-in tool does not expose its underlying model ID.")
    for job in plan:
        if job["id"] not in imports:
            if args.model == "image_gen":
                parser.error("image_gen runs through the agent tool; use --plan then --import-image")
            if job["background"] == "transparent" and args.model != "gpt-image-1.5":
                parser.error("True-transparent CLI assets require an explicit --model gpt-image-1.5 choice; no automatic model switch")
            if not args.cli.is_file():
                parser.error(f"Imagegen skill CLI missing: {args.cli}")
    failures = []
    for job in plan:
        asset_id = job["id"]
        record = report["assets"].setdefault(asset_id, {"attempts": 0})
        import_hash = hashlib.sha256(Path(imports[asset_id]).expanduser().read_bytes()).hexdigest() if asset_id in imports else None
        same_import = import_hash is not None and record.get("import_sha256") == import_hash
        if not same_import and record.get("attempts", 0) >= 3:
            print(f"Rejected: {asset_id}; maximum of three attempts reached", file=sys.stderr)
            failures.append(asset_id)
            continue
        remaining = 1 if same_import else min(args.attempts, 3 - record.get("attempts", 0))
        for _ in range(remaining):
            if not same_import:
                record["attempts"] = record.get("attempts", 0) + 1
            try:
                if asset_id in imports:
                    source = Path(imports[asset_id]).expanduser()
                else:
                    source = ROOT / "output/imagegen" / f"{asset_id}-{record['attempts']}.png"
                    source.parent.mkdir(parents=True, exist_ok=True)
                    cmd = [sys.executable, str(args.cli), "generate", "--model", args.model,
                           "--prompt", job["prompt"], "--size", job["size"], "--quality", "high",
                           "--background", job["background"], "--out", str(source)]
                    subprocess.run(cmd, check=True)
                size = finish(source, job, ROOT / job["file"])
                manifest(job, args.model, size)
                if import_hash:
                    record["import_sha256"] = import_hash
                record.update(status="needs_style_review", file=job["file"], model=args.model,
                              source_size_note="Built-in tool dimensions may differ; final size validated")
                print(f"Ready for review: {job['file']} ({size[0]}x{size[1]})")
                break
            except (OSError, ValueError, subprocess.CalledProcessError) as exc:
                report["rejected"].append(dict(id=asset_id, attempt=record["attempts"], reason=str(exc), kind="format_or_generation"))
                record["status"] = "rejected"
                if asset_id in imports:  # Re-importing the same bad file cannot fix it.
                    failures.append(asset_id)
                    break
        else:
            failures.append(asset_id)
        write_json(REPORT, report)
    print(f"Contact sheet: {sheet()}")
    if args.sheet_all:
        full_sheets(all_jobs)
    return 1 if failures else 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, ImportError) as error:
        print(f"gen-assets: {error}", file=sys.stderr)
        raise SystemExit(1)
