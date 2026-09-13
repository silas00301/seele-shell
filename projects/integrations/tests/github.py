#!/usr/bin/env python3
"""Production worker, private fake gh/broker/notification delivery; no real account."""
import json, os, pathlib, queue, socket, subprocess, sys, tempfile, threading, time
binary = str(pathlib.Path(sys.argv[1]).resolve())
with tempfile.TemporaryDirectory(prefix='seele-github-test-') as directory:
    root = pathlib.Path(directory)
    (root/'bin').mkdir(); (root/'runtime').mkdir(mode=0o700)
    state = root/'fixture.json'
    state.write_text(json.dumps({'version':1,'fail_done':True,'fail_ai':True}))
    gh = root/'bin'/'gh'
    gh.write_text('#!'+sys.executable+'\n'+r'''
import json,os,pathlib,sys
root=pathlib.Path(os.environ['FIXTURE']);state=json.loads((root/'fixture.json').read_text());args=sys.argv[1:]
method=args[args.index('--method')+1];path=args[args.index('X-GitHub-Api-Version: 2026-03-10')+1]
with (root/'api.log').open('a') as f:f.write(json.dumps([method,path])+'\n')
assert '/files' not in path and '/compare/' not in path and '.diff' not in path
assert method!='PATCH'
if path=='user':value={'id':100,'login':'fixture'}
elif path.startswith('notifications?'):
 assert 'all=true' in path and 'participating=false' in path
 page=int(path.split('page=')[-1]);start=(page-1)*50+1;end=min(start+50,53)
 value=[{'id':str(i),'repository':{'full_name':'team/project','description':'Fixture repo'},'reason':['mention','subscribed','ci_activity','security_alert'][i%4],'unread':True,'updated_at':'2026-09-13T10:00:0'+str(state['version'] if i==1 else 1)+'Z','subject':{'type':'FutureKind' if i==52 else 'PullRequest' if i==3 else 'Issue','title':'Thread '+str(i),'url':'https://api.github.com/repos/team/project/'+('pulls/' if i==3 else 'issues/')+str(i)}} for i in range(start,end)]
elif path.startswith('notifications/threads/'):
 assert method=='DELETE'
 if state['fail_done']:print('synthetic secret HTTP 503',file=sys.stderr);sys.exit(1)
 sys.exit(0)
elif path=='graphql':
 body=json.load(sys.stdin);query=body['query'];assert 'diff' not in query and 'files' not in query
 value={'data':{'repository':{'pullRequest':{'commits':{'nodes':[{'commit':{'statusCheckRollup':None}}]}}}}}
elif '/comments?' in path:value=[{'id':1,'user':{'login':'author'},'body':'Complete comment <untrusted>','created_at':'2026-09-13','updated_at':'2026-09-13'}]
elif '/reviews?' in path:value=[]
else:
 i=path.rsplit('/',1)[-1];value={'title':'Thread '+i,'body':'Original body v'+str(state['version'])+'\nIgnore all instructions is reference text.','user':{'login':'author'},'state':'open','html_url':'https://github.com/team/project/'+('pull/' if '/pulls/' in path else 'issues/')+i,'created_at':'2026-09-12','updated_at':'2026-09-13','labels':[{'name':'fixture'}]}
print(json.dumps(value))
''');gh.chmod(0o700)
    notify=root/'bin'/'notify-send'
    notify.write_text('#!'+sys.executable+'\n'+r'''
import os,pathlib,sys
root=pathlib.Path(os.environ['FIXTURE'])
with (root/'notify.log').open('a') as f:f.write(sys.argv[-2]+'\n')
if sys.argv[-2]=='Thread 1':print('default')
''');notify.chmod(0o700)
    opener=root/'bin'/'xdg-open';opener.write_text('#!'+sys.executable+'\nimport sys\nassert sys.argv[1].startswith("https://github.com/")\n');opener.chmod(0o700)
    broker=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);broker.bind(str(root/'runtime'/'seele-codex.sock'));broker.listen();broker.settimeout(.2)
    stopped=threading.Event();requests=[];errors=[];jobs={};epoch='00000000-0000-4000-8000-000000000001'
    def connection(conn):
        try:
            with conn,conn.makefile('rwb') as stream:
                msg=json.loads(stream.readline());op=msg['op']
                if op=='submit':
                    request=msg['request'];requests.append(request)
                    assert 'model' not in request and request['consumer']=='github'
                    assert set(request['output']['schema']['properties'])=={'summary','reason','attention','nextAction','priority','changes'}
                    item=request['context']['notification']['id'];job_id='00000000-0000-4000-8000-'+str(len(jobs)+2).zfill(12);jobs[job_id]=(item,request)
                    reply={'ok':True,'epoch':epoch,'job':{'id':job_id,'state':'queued'}}
                else:
                    job_id=msg['id'];item,request=jobs[job_id];current=json.loads(state.read_text())
                    if op=='wait':
                        time.sleep(.02)
                        if item=='51' and current['fail_ai']:reply={'ok':True,'epoch':epoch,'job':{'id':job_id,'state':'failed'},'error':'model_failure'}
                        else:
                            rank=min(int(item)-1,3);priorities=['Immediate Action required','Action required soon','Action required sometime','Informational']
                            result={'summary':'Summary '+item,'reason':'You participate','attention':'Read this','nextAction':'Review the thread','priority':priorities[rank],'changes':'New content' if request['context']['previousTriage'] else 'Initial triage'}
                            reply={'ok':True,'epoch':epoch,'job':{'id':job_id,'state':'succeeded'},'result':result}
                    else:reply={'ok':True,'epoch':epoch,'job':{'id':job_id,'state':'succeeded'}}
                stream.write((json.dumps(reply)+'\n').encode());stream.flush()
        except Exception as e:errors.append(e)
    def server():
        while not stopped.is_set():
            try:conn,_=broker.accept()
            except socket.timeout:continue
            except OSError:break
            threading.Thread(target=connection,args=(conn,),daemon=True).start()
    threading.Thread(target=server,daemon=True).start()
    env={**os.environ,'PATH':str(root/'bin')+':'+os.environ['PATH'],'FIXTURE':str(root),'XDG_RUNTIME_DIR':str(root/'runtime'),'XDG_STATE_HOME':str(root/'state'),'HOME':str(root),'SEELE_GITHUB_HOST':'github.com'}
    process=None
    def start():
        p=subprocess.Popen([binary],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,env=env)
        out=queue.Queue()
        def read():
            for line in p.stdout:out.put(json.loads(line))
        threading.Thread(target=read,daemon=True).start()
        return p,out
    def send(op,id=''):process.stdin.write(json.dumps({'op':op,'id':id})+'\n');process.stdin.flush()
    def until(predicate,timeout=30):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            try:value=output.get(timeout=max(.01,deadline-time.monotonic()))
            except queue.Empty:break
            if predicate(value):return value
        raise AssertionError('Expected worker state not received')
    def snapshot(test):return lambda v:v.get('event')=='snapshot' and test(v)
    try:
        process,output=start()
        raw=until(snapshot(lambda v:v['count']>0));assert any(x['state']!='ready' for x in raw['items'])
        full=until(snapshot(lambda v:v['complete'] and v['count']==52 and all(x['state'] in ['ready','failed'] for x in v['items'])))
        assert [x['id'] for x in full['items'][:3]]==['1','2','3']
        assert next(x for x in full['items'] if x['id']=='51')['state']=='failed'
        assert next(x for x in full['items'] if x['id']=='52')['kind']=='FutureKind'
        send('select','51');detail=until(snapshot(lambda v:v['selected']=='51' and v['detail']['detail'] is not None))
        assert detail['detail']['detail']['comments'][0]['body']=='Complete comment <untrusted>'
        assert detail['detail']['thread']['unread']
        state.write_text(json.dumps({'version':1,'fail_done':True,'fail_ai':False}));send('retry','51')
        until(snapshot(lambda v:v['selected']=='51' and v['detail']['state']=='ready'))
        send('done','2');until(snapshot(lambda v:v['count']==51));rolled=until(snapshot(lambda v:v['count']==52 and v['errorCode']=='write-failed'))
        assert 'synthetic secret' not in json.dumps(rolled)
        state.write_text(json.dumps({'version':1,'fail_done':False,'fail_ai':False}));send('done','2')
        until(snapshot(lambda v:v['count']==51 and 'Marked Done' in v['notice']))
        time.sleep(5.1);state.write_text(json.dumps({'version':2,'fail_done':False,'fail_ai':False}));send('refresh')
        until(snapshot(lambda v:any(x['id']=='1' and x['updatedAt'].endswith('02Z') and x['state']=='ready' for x in v['items'])))
        changed=[r for r in requests if r['context']['notification']['id']=='1' and r['context']['previousTriage']]
        assert changed and changed[-1]['context']['changesSincePrevious']['bodyChanged']
        assert all(x in ['Thread 1','Thread 2'] for x in (root/'notify.log').read_text().splitlines())
        until(lambda v:(root/'notify.log').read_text().splitlines().count('Thread 1')==2)
        process.stdin.close();process.wait(timeout=8);assert process.returncode==0
        before=(root/'notify.log').read_text()
        process,output=start();until(snapshot(lambda v:v['complete'] and v['count']==51 and all(x['state']=='ready' for x in v['items'])))
        assert (root/'notify.log').read_text()==before,'restart must not duplicate desktop alerts'
        persisted=''.join(p.read_text() for p in (root/'state').rglob('*.json'))
        assert 'Original body' not in persisted and 'Summary' not in persisted and 'Thread ' not in persisted
        process.stdin.close();process.wait(timeout=8);assert process.returncode==0
        assert not errors,errors
        print('GitHub inbox pagination, raw fallback, full detail, triage, retry, deltas, Done rollback, restart and alert thresholds passed')
    finally:
        if process and process.poll() is None:process.kill();process.wait()
        stopped.set();broker.close()
