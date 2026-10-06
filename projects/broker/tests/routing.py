"""Real broker lifecycle with synthetic quota/Claude/wallet; no remote I/O."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import protocol

with tempfile.TemporaryDirectory(prefix='seele-routing-fixture-') as temporary:
    root = Path(temporary)
    quota = root / 'quota'
    claude = root / 'claude'
    wallet = root / 'wallet'
    calls = root / 'calls'
    state = root / 'state.json'
    header = '#!' + sys.executable + '\n'
    quota.write_text(header + f'''import json,sys
assert sys.argv[1:]==['usage','--provider','codex','--json']
state=json.load(open({str(state)!r}))
print(json.dumps(state['quota']))
''')
    wallet.write_text(header + f'''import json,sys
assert sys.argv[1:]==['lookup','application','seele-codex','account','claude']
if not json.load(open({str(state)!r}))['wallet']: sys.exit(1)
print('SYNTHETIC-BROKER-KEY')
''')
    claude.write_text(header + f'''import json,os,shlex,subprocess,sys,time
from pathlib import Path
args=sys.argv[1:]
state=json.load(open({str(state)!r}))
if args==['--help']:
 for flag in ('--bare --tools --disallowedTools --strict-mcp-config --no-session-persistence --setting-sources' if state.get('supported',True) else '--tools').split(): print(flag)
 sys.exit(0)
assert '--bare' in args and '--no-session-persistence' in args
assert args[args.index('--tools')+1]==''
assert args[args.index('--disallowedTools')+1]=='*'
assert args[args.index('--mcp-config')+1]=='{{"mcpServers":{{}}}}'
assert args[args.index('--setting-sources')+1]==''
assert os.environ.get('PRIVATE_INTEGRATION_TOKEN') is None
assert os.environ.get('ANTHROPIC_API_KEY') is None
assert not list(Path.cwd().iterdir())
assert not list(Path(os.environ['HOME']).iterdir())
helper=json.loads(args[args.index('--settings')+1])['apiKeyHelper']
assert subprocess.check_output(shlex.split(helper)).decode()=='SYNTHETIC-BROKER-KEY'
request=json.load(sys.stdin)
assert request['task']=='Private prompt' and request['context']=={{'value':1}}
assert request['outputSchema']=={{'type':'integer'}}
model=args[args.index('--model')+1]
with open({str(calls)!r},'a') as f: f.write(model+'\\n')
if state.get('slow'): time.sleep(30)
print(json.dumps({{'type':'system','subtype':'init','tools':['Bash'] if state.get('tool') else [],'mcp_servers':[]}}))
if model in state.get('fail',[]):
 print(json.dumps({{'type':'result','subtype':'error_during_execution','is_error':True}}))
else:
 print(json.dumps({{'type':'result','subtype':'success','is_error':False,'result':'42','usage':{{'input_tokens':2,'output_tokens':1}}}}))
''')
    for binary in [quota, wallet, claude]: binary.chmod(0o700)
    environment = dict(SEELE_BROKER_CODEXBAR=str(quota), SEELE_BROKER_CLAUDE=str(claude), SEELE_BROKER_SECRET_TOOL=str(wallet))
    def run(used, *, fail=(), wallet=True, supported=True, tool=False):
        calls.write_text('')
        state.write_text(json.dumps(dict(quota=[] if used is None else [{'provider':'codex','usage':{'primary':{'usedPercent':used}}}],wallet=wallet,fail=list(fail),supported=supported,tool=tool)))
        with protocol.broker(environment_overrides=environment) as (_, path, _, _):
            reply = protocol.rpc(path, {'op':'submit','request':protocol.payload()})
            result = protocol.rpc(path, dict(op='wait',id=reply['job']['id'],epoch=reply['epoch']))
            return result, calls.read_text().splitlines()
    for used, reason in [(0,'codex_quota_available'), (80,'codex_quota_available'), (None,'codex_quota_unknown')]:
        result, selected = run(used)
        assert result.get('result')==42 and result['job']['model']=='gpt-5.6-luna', result
        assert result['job']['selectionReason']==reason and selected==[], (result, selected)
    for fail, model, expected in [((), 'haiku',['haiku']), (['haiku'],'sonnet',['haiku','sonnet']), (['haiku','sonnet'],'opus',['haiku','sonnet','opus'])]:
        result, selected = run(100, fail=fail)
        assert result.get('result')==42 and result['job']['model']=='claude:'+model, result
        assert result['job']['selectionReason']=='codex_quota_exhausted' and selected==expected, (result,selected)
    for options, error, expected in [(dict(wallet=False),'authentication_unavailable',[]),(dict(supported=False),'isolation_failure',[]),(dict(tool=True),'isolation_failure',['haiku'])]:
        result, selected = run(100, **options)
        assert result['job']['error']==error and 'result' not in result and selected==expected, (result,selected)
    calls.write_text('')
    state.write_text(json.dumps(dict(quota=[{'provider':'codex','usage':{'primary':{'usedPercent':100}}}],wallet=True,slow=True)))
    with protocol.broker(environment_overrides=environment) as (_, path, _, _):
        reply=protocol.rpc(path, {'op':'submit','request':protocol.payload()})
        protocol.eventually(lambda:calls.read_text().splitlines()==['haiku'])
        identity=dict(id=reply['job']['id'],epoch=reply['epoch'])
        assert protocol.rpc(path,dict(op='cancel',**identity))['ok']
        assert protocol.rpc(path,dict(op='wait',**identity))['job']['state']=='cancelled'
        assert calls.read_text().splitlines()==['haiku']
print('Quota routing: positive/unknown Codex, exhausted Haiku/Sonnet/Opus, wallet and tool isolation passed')
