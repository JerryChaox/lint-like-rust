import json
from pathlib import Path
import tempfile
import unittest
from prepare_repair_eval import prepare

REPO = Path(__file__).resolve().parents[1]


class PreparationTests(unittest.TestCase):
    def test_equal_inputs_and_no_answer_or_condition_in_worker_payload(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp) / 'run'
            records = prepare(REPO, root, 1)
            self.assertEqual(len(records), 6)
            for task in {r['task_id'] for r in records}:
                pair = [r for r in records if r['task_id'] == task]
                self.assertEqual({r['arm'] for r in pair}, {'baseline', 'llr'})
                for name in ['task.py', 'task.md']:
                    self.assertEqual(*[(root/'workers'/r['attempt_id']/name).read_bytes() for r in pair])
            for worker in (root/'workers').iterdir():
                self.assertEqual({p.name for p in worker.iterdir()}, {'task.py', 'task.md'})
            state = json.loads((root/'controller/runs.json').read_text())
            self.assertFalse(state['isolation_verified'])
            self.assertTrue(all(r['semantic_verdict'] is None for r in records))
            with self.assertRaises(FileExistsError):
                prepare(REPO, root, 1)

    def test_changed_fixture_rejected_before_output(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root/'eval/repair').mkdir(parents=True)
            (root/'tests/corpus_v2').mkdir(parents=True)
            (root/'tests/corpus_v2/input.py').write_text('changed')
            (root/'eval/repair/smoke-manifest.json').write_text(json.dumps({'tasks': [{
                'source_fixture': 'tests/corpus_v2/input.py', 'sha256': 'wrong'}]}))
            with self.assertRaisesRegex(ValueError, 'hash mismatch'):
                prepare(root, root/'out', 1)
            self.assertFalse((root/'out').exists())


if __name__ == '__main__':
    unittest.main()
