#!/usr/bin/env python3
"""Self-contained in-memory counterexample; never imports or runs corpus Python.

Demonstrates why a dict annotation and standard json behavior do not imply
callback-free serialization. Frame inspection is ordinary Python behavior.
"""
import io
import json
import sys
import platform
from pathlib import Path

class ClosingRecord(dict):
    def items(self):
        frame=sys._getframe()
        try:
            while frame:
                candidate=frame.f_locals.get('f')
                if type(candidate) is io.StringIO:
                    candidate.close()
                    break
                frame=frame.f_back
        finally:
            del frame
        return super().items()

def append(record: dict):
    with io.StringIO() as f:
        f.write(json.dumps(record,ensure_ascii=False)+'\n')
        return f.getvalue()

normal=append({'key':'value'})=='{"key": "value"}\n'
raised=False
try:
    append(ClosingRecord(key='value'))
except ValueError:
    raised=True
assert normal and raised
out=Path('reports/v2/json-callback-boundary.json')
out.write_text(json.dumps({'python':platform.python_version(),'stdlib_json_unchanged':True,'plain_dict_write_succeeds':normal,'dict_subclass_callback_closes_local_stream':raised,'scanned_source_executed':False},indent=2)+'\n')
print('Standard json + dict annotation allows callback effects; independent in-memory counterexample confirmed.')
