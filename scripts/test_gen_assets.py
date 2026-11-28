"""Offline checks for the asset pipeline; never calls an image API."""
import importlib.util
from pathlib import Path
import tempfile
import sys
sys.dont_write_bytecode = True
import unittest

from PIL import Image, ImageDraw

spec = importlib.util.spec_from_file_location("gen_assets", Path(__file__).with_name("gen-assets.py"))
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)


class AssetsPipelineTest(unittest.TestCase):
    def test_designer_spec_has_all_assets_and_prompts(self):
        jobs = gen.jobs(gen.ROOT / "assets-to-generate.md")
        self.assertEqual(len(jobs), 27)
        self.assertEqual(sum(j["kind"] == "miniature" for j in jobs), 18)
        self.assertTrue(all(j["prompt"] and j["file"].startswith("assets/") for j in jobs))

    def test_transparent_miniature_keeps_contact_and_clear_border(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory) / "raw.png", Path(directory) / "mini.png"
            raw = Image.new("RGBA", (1024, 1024))
            ImageDraw.Draw(raw).rectangle((150, 200, 900, 940), fill=(150, 120, 80, 255))
            raw.save(source)
            self.assertEqual(gen.finish(source, {"kind": "miniature"}, output), (512, 512))
            with Image.open(output) as mini:
                self.assertEqual(mini.mode, "RGBA")
                alpha = mini.getchannel("A")
                solid = alpha.point(lambda a: 255 if a >= 128 else 0).getbbox()
                self.assertLess(abs(solid[3] / 512 - 0.88), 0.01)
                self.assertTrue(all(alpha.getpixel(p) == 0 for i in range(512)
                                    for p in ((i, 0), (i, 511), (0, i), (511, i))))

    def test_bad_miniature_does_not_replace_previous_output(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory) / "opaque.png", Path(directory) / "mini.png"
            Image.new("RGB", (1024, 1024), "red").save(source)
            output.write_bytes(b"previous file")
            with self.assertRaisesRegex(ValueError, "transparency"):
                gen.finish(source, {"kind": "miniature"}, output)
            self.assertEqual(output.read_bytes(), b"previous file")

    def test_briefing_is_cropped_to_webp_template(self):
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory) / "raw.png", Path(directory) / "briefing/pilot.webp"
            Image.new("RGB", (1536, 1024), "tan").save(source)
            self.assertEqual(gen.finish(source, {"kind": "illustration"}, output), (1600, 900))
            with Image.open(output) as result:
                self.assertEqual(result.format, "WEBP")

    def test_texture_seam_metric_rejects_gradient_and_accepts_uniform_ground(self):
        image = Image.new("RGB", (256, 256))
        for x in range(256):
            ImageDraw.Draw(image).line((x, 0, x, 255), fill=(x, x, x))
        self.assertGreater(gen.terrain_seams(image)[0], 1.5)
        self.assertEqual(gen.terrain_seams(Image.new("RGB", (256,256), "tan")), (1.0,1.0))


if __name__ == "__main__":
    unittest.main()
