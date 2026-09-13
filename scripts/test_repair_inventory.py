import tempfile
import unittest
from pathlib import Path
from inventory_repair_candidates import inventory


class InventoryTests(unittest.TestCase):
    def test_module_separation_exclusion_and_no_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);apps=root/'apps';apps.mkdir()
            code="raise RuntimeError('must never execute')\ndef read(path):\n    with open(path) as f:\n        return f.read()\n"
            (apps/'a.py').write_text(code+"\ndef other(path):\n    with open(path) as g:\n        return g.read()\n")
            (apps/'b.py').write_text(code)
            (apps/'.claude/worktrees/copy').mkdir(parents=True)
            (apps/'.claude/worktrees/copy/duplicate.py').write_text(code)
            (apps/'excluded.py').write_text(code)
            (apps/'broken.py').write_text('def :')
            manifest={'cases':[{'source':{'path':'apps/excluded.py'}}]}
            result=inventory(root,manifest)
            self.assertEqual({c['path'] for c in result['candidates']},{'apps/a.py','apps/b.py'})
            self.assertEqual(len(result['candidates']),2)
            self.assertEqual(result['admitted_tasks'],0)
            self.assertEqual(len(result['parse_failures']),1)
            self.assertEqual(result,inventory(root,manifest))


if __name__=='__main__':
    unittest.main()
