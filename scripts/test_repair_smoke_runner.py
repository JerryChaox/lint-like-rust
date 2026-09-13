import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from run_repair_smoke import invoke


class RunnerTests(unittest.TestCase):
    def run_events(self, extra=(), usage=True):
        events=[{'type':'thread.started'},*extra,{'type':'item.completed','item':{
            'type':'agent_message','text':json.dumps({'source':'pass'})}}]
        if usage:events.append({'type':'turn.completed','usage':{'input_tokens':20,'output_tokens':3}})
        response=SimpleNamespace(returncode=0,stdout='\n'.join(json.dumps(e) for e in events),stderr='')
        with tempfile.TemporaryDirectory() as tmp, patch('run_repair_smoke.subprocess.run',return_value=response):
            return invoke('synthetic',{},Path(tmp)/'result.json')

    def test_tool_event_is_invalid_even_with_good_final(self):
        r=self.run_events([{'type':'item.completed','item':{'type':'command_execution','command':'cat answer'}}])
        self.assertEqual(r['status'],'invalid')

    def test_host_error_is_invalid(self):
        self.assertEqual(self.run_events([{'type':'item.completed','item':{'type':'error','message':'host unavailable'}}])['status'],'invalid')

    def test_missing_usage_and_unknown_event_fail_closed(self):
        self.assertEqual(self.run_events(usage=False)['status'],'invalid')
        self.assertEqual(self.run_events([{'type':'unknown.tool.event'}])['status'],'invalid')

    def test_plain_response_is_only_observed_not_enforced(self):
        result=self.run_events()
        self.assertEqual(result['status'],'observed_text_only')
        self.assertFalse(result['enforced_isolation'])


if __name__=='__main__':unittest.main()
