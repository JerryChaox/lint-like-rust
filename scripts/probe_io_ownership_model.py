#!/usr/bin/env python3
"""Conformance probe for Python's in-memory IO API, never imports scanned code."""
import io
import json
from pathlib import Path
import sys

raw=io.BytesIO(b'payload')
text=io.TextIOWrapper(raw,'utf-8')
assert raw.read(1)==b'p'  # Legal Python; llr intentionally labels a policy restriction.
text.close()
assert raw.closed
raw=io.BytesIO(b'payload')
text=io.TextIOWrapper(raw,'utf-8')
recovered=text.detach()
assert recovered is raw and not recovered.closed
try:
    text.read()
except ValueError:
    pass
else:
    raise AssertionError('Detached text wrapper unexpectedly usable')
try:
    text.close()
except ValueError:
    pass
else:
    raise AssertionError('Detached wrapper close unexpectedly succeeded')
assert not recovered.closed
recovered.close()
raw=io.BytesIO()
try:
    io.TextIOWrapper(raw,'llr-not-a-codec')
except LookupError:
    pass
else:
    raise AssertionError('Unknown codec accepted')
assert not raw.closed
raw.close()
result={'python':sys.version.split()[0], 'alias_read_is_legal_python':True,
        'wrapper_close_closes_buffer':True, 'detach_returns_same_buffer_and_invalidates_wrapper':True,
        'encoding_lookup_failure_does_not_close_buffer':True, 'detached_wrapper_close_does_not_close_buffer':True, 'scanned_code_executed':False}
Path('reports/v2/io-ownership-model-conformance.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result))
