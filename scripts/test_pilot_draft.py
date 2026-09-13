"""Structural checks on generated drafts, not independent correctness verdicts."""
import ast
import hashlib
import json
from pathlib import Path
import unittest

ROOT=Path(__file__).resolve().parents[1]/'eval/repair/pilot-draft'


class DraftTests(unittest.TestCase):
    def test_provenance_and_mutation_shape(self):
        for name in ['marker_validation','json_lines','ready_payload']:
            record=json.loads((ROOT/'controller'/name/'record.json').read_text())
            task=(ROOT/name/'task.py').read_text()
            reference=(ROOT/'controller'/name/'reference.py').read_text()
            self.assertEqual(hashlib.sha256(task.encode()).hexdigest(),record['input_sha256'])
            self.assertEqual(hashlib.sha256(reference.encode()).hexdigest(),record['reference_sha256'])
            self.assertFalse(record['is_existing_museon_bug'])
            self.assertIsNone(record['independent_verdict'])
            fn=next(n for n in ast.parse(task).body if isinstance(n,ast.FunctionDef))
            contexts=[n for n in ast.walk(fn) if isinstance(n,ast.With)]
            self.assertEqual(len(contexts),1)
            if record['condition']=='error':
                self.assertIsInstance(contexts[0].body[0],ast.Pass)
                self.assertEqual(reference.replace(record['mutation']['before'],record['mutation']['after'],1),task)
            else:
                self.assertEqual(reference,task)


if __name__=='__main__':
    unittest.main()
