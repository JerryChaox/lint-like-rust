#!/usr/bin/env python3
"""In-memory standard-library model conformance only; never loads scanned code."""
import io
import json
import platform
from pathlib import Path

def raises(exception, action):
    try:
        action()
    except exception:
        return True
    return False

raw = io.BytesIO(b'abc')
view = raw.getbuffer()
alias = view
facts = {'owner_read_is_legal_python': raw.read() == b'abc'}
facts['close_export_raises_without_closing'] = raises(BufferError, raw.close) and not raw.closed
child = view.toreadonly()
view[0] = 65
facts['readonly_child_is_not_immutable_snapshot'] = child.tobytes() == b'Abc'
facts['readonly_write_rejected'] = raises(TypeError, lambda: child.__setitem__(0, 66))
view.release()
alias.release()
facts['alias_release_idempotent'] = raises(ValueError, alias.tobytes)
facts['child_survives_parent_release'] = child.tobytes() == b'Abc'
facts['child_still_blocks_close'] = raises(BufferError, raw.close) and not raw.closed
facts['released_enter_raises'] = raises(ValueError, view.__enter__)
child.release()
raw.close()
facts['all_views_released_owner_closes'] = raw.closed
with memoryview(b'abc') as scoped:
    assert scoped.tobytes() == b'abc'
facts['context_exit_releases_view'] = raises(ValueError, scoped.tobytes)
assert all(facts.values()), facts
out = Path('reports/v2/buffer-view-model-conformance.json')
out.write_text(json.dumps({'python': platform.python_version(), 'checks': facts}, indent=2)+'\n')
print(f'Buffer view model: {len(facts)} stdlib conformance checks passed; no scanned code executed.')
