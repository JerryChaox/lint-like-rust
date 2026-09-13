import copy
import unittest
from type_facts import byte_offset, normalize, source_hash


class FactsTest(unittest.TestCase):
    def setUp(self):
        self.source = "中文😀; p.open()\r\n"
        self.query = {'uri': 'file:///sample.py', 'range': {
            'start': {'line': 0, 'character': 6}, 'end': {'line': 0, 'character': 12}}}
        self.binding = dict(query_range=self.query['range'], snapshot=5, uri=self.query['uri'], protocol='0.4.1',
                            document_sha256=source_hash(self.source), provider_sha256='provider',
                            stubs_sha256='stubs', configuration_sha256='configuration')
        self.function = {'id': 0, 'kind': 2, 'declaration': {'name': 'open', 'node': {
            'uri': 'file:///pathlib.pyi', 'range': self.query['range']}}}

    def run_fact(self, response=None, **kwargs):
        args = dict(source=self.source, query=self.query, binding=self.binding,
                    expected_binding=copy.deepcopy(self.binding))
        args.update(kwargs)
        return normalize(response or {'result': self.function}, **args)

    def test_unicode_crlf_and_invalid_positions(self):
        fact = self.run_fact()
        self.assertEqual(self.source.encode()[fact['span']['start']:fact['span']['end']], b'p.open')
        self.assertEqual(byte_offset(self.source, {'line': 1, 'character': 0}), len(self.source.encode()))
        for line, char in [(0, 3), (0, 99), (-1, 0), (3, 0)]:
            with self.assertRaises(ValueError):
                byte_offset(self.source, {'line': line, 'character': char})

    def test_every_identity_change_rejects_reuse(self):
        for key in self.binding:
            changed = {**self.binding, key: 'changed'}
            self.assertEqual(self.run_fact(expected_binding=changed)['reasons'], ['binding_mismatch'])
        self.assertEqual(self.run_fact(source=self.source + '# edit')['status'], 'unknown')

    def test_same_name_is_not_same_target(self):
        other = copy.deepcopy(self.function)
        other['declaration']['node']['uri'] = 'file:///dialog.py'
        a, b = self.run_fact(), self.run_fact({'result': other})
        self.assertNotEqual(a['candidates'], b['candidates'])
        self.assertEqual(a['dispatch'], 'nominal_candidates_only')

    def test_reference_resolution_is_response_local(self):
        root = {'id': 2, 'kind': 7, 'overloads': [self.function, {'kind': 9, 'typeReferenceId': 0}]}
        self.assertEqual(len(self.run_fact({'result': root})['candidates']), 1)
        missing = self.run_fact({'result': {'kind': 9, 'typeReferenceId': 0}})
        self.assertEqual(missing['status'], 'unknown')
        self.assertFalse(missing['candidates'])

    def test_unknown_and_partial_overloads_stay_unknown(self):
        unknown = {'kind': 0, 'name': 'unknown'}
        for value in [unknown, {'kind': 7, 'overloads': [self.function, unknown]}]:
            self.assertEqual(self.run_fact({'result': value})['status'], 'unknown')

    def test_stale_error_not_empty_success(self):
        self.assertEqual(self.run_fact({'error': {'code': -32802}})['reasons'], ['stale_snapshot'])


if __name__ == '__main__':
    unittest.main()
