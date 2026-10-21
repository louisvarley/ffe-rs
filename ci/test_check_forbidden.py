import subprocess
import unittest
from pathlib import Path

from ci.check_forbidden import violation


ROOT = Path(__file__).resolve().parents[1]
PRIVATE_PATHS = (
    "game/MISSION.DAT",
    "legacy/reference/ffe.asm",
    "jjffe-master/ffe.asm",
    "ffed3d-master/resources/models/models.fx",
    "Frontier-First-Encounters_DOS_EN_Floppy-version/Elite3/IMAGE.DAT",
    "Frontier-First-Encounters_DOS_EN_ISO-Version/disc/First_Encounters.bin",
    "Frontier-First-Encounters_DOS_EN_Patch-106-for-CD-Version/BLD_INFO.CFG",
    "Frontier-First-Encounters_Patch_DOS_EN_Patch-106d-for-Floppy-Version/LANGRC.RES",
    "data.bin",
    "data.bin.tmp",
    "upscaled.bin",
    "upscaled.bin.tmp",
    "to-be-upscaled/manifest.json",
    "upscaled/hud.png",
    "extracted/models.json",
    "saves/commander.json",
    "tests/golden/frames/reference.png",
)


class ForbiddenFilesTests(unittest.TestCase):
    def test_private_inputs_and_derivatives_are_rejected(self):
        for path in PRIVATE_PATHS:
            with self.subTest(path=path):
                self.assertIsNotNone(violation(path))

    def test_windows_separators_and_case_cannot_bypass_guard(self):
        for path in PRIVATE_PATHS:
            with self.subTest(path=path):
                self.assertIsNotNone(violation(path.upper().replace("/", "\\")))

    def test_code_docs_and_nonreconstructable_goldens_are_allowed(self):
        for path in (
            "CLAUDE.MD",
            "README.md",
            ".gitattributes",
            "ci/test_check_forbidden.py",
            "crates/ffe-core/src/lib.rs",
            "docs/GAME_FILES.md",
            "tests/golden/rng.json",
            "tests/synthetic/texture.png",
        ):
            with self.subTest(path=path):
                self.assertIsNone(violation(path))

    def test_existing_asset_and_capture_rules_still_apply(self):
        for path in ("elsewhere/asset.BMP", "elsewhere/game.EXE", "FRAME_001.PNG"):
            with self.subTest(path=path):
                self.assertIsNotNone(violation(path))

    def test_private_paths_are_also_git_ignored(self):
        result = subprocess.run(
            ["git", "check-ignore", "--no-index", "-z", "--stdin"],
            input=("\0".join(PRIVATE_PATHS) + "\0").encode("utf-8"),
            capture_output=True,
            cwd=ROOT,
            check=True,
        )
        ignored = result.stdout.decode("utf-8").rstrip("\0").split("\0")
        self.assertEqual(set(ignored), set(PRIVATE_PATHS))


if __name__ == "__main__":
    unittest.main()
