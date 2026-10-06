import fcntl,os,pty,select,signal,subprocess,sys,tempfile,termios,time
from pathlib import Path
binary=str(Path(sys.argv[1]).resolve());fish=str(Path(sys.argv[2]).resolve())
with tempfile.TemporaryDirectory() as directory:
    root=Path(directory); tools=root/'bin';tools.mkdir();(tools/'fish').symlink_to(fish)
    original=root/'original';original.write_text('preserve me'); parent=root/'caller';parent.mkdir(); temps=root/'temps';temps.mkdir()
    def session(cancel):
        receipt=root/('cancel-receipt' if cancel else 'exit-receipt')
        master,slave=pty.openpty()
        def terminal(): os.setsid();fcntl.ioctl(0,termios.TIOCSCTTY,0)
        env={**os.environ,'PATH':str(tools)+os.pathsep+os.environ['PATH'],'TMPDIR':str(temps),'TERM':'dumb','SCRATCH_RECEIPT':str(receipt),'SCRATCH_ORIGINAL':str(original)}
        child=subprocess.Popen([binary],cwd=parent,env=env,stdin=slave,stdout=slave,stderr=slave,preexec_fn=terminal);os.close(slave)
        output=b''
        def pump_until(predicate):
            nonlocal output
            deadline=time.monotonic()+10
            while not predicate():
                assert time.monotonic()<deadline,output[-3000:]
                if select.select([master],[],[],0.05)[0]:
                    try:output+=os.read(master,65536)
                    except OSError:break
        try:
            pump_until(lambda:b'scratch>' in output)
            os.write(master,b'''printf '%s\\n' "$PWD" "$fish_private_mode" "$XDG_CONFIG_HOME" > "$SCRATCH_RECEIPT"; printf test > owned; ln -s "$SCRATCH_ORIGINAL" link\r''')
            pump_until(lambda:receipt.exists() and len(receipt.read_text().splitlines())>=3)
            lines=receipt.read_text().splitlines();assert len(lines)>=3,lines
            workspace=Path(lines[0]);assert workspace.parent==temps and workspace.name.startswith('seele-scratch-')
            assert lines[1] and Path(lines[2]).is_relative_to(workspace)
            pump_until(lambda:(workspace/'owned').exists())
            assert workspace.stat().st_mode&0o777==0o700
            assert (workspace/'owned').stat().st_mode&0o777==0o600
            if cancel:
                os.write(master,b'read scratch_wait\r');child.send_signal(signal.SIGTERM)
            else:os.write(master,b'exit 7\r')
            pump_until(lambda:child.poll() is not None)
            code=child.wait(timeout=5);assert code==(1 if cancel else 7),(code,output[-3000:])
            assert not workspace.exists() and not list(temps.iterdir())
            assert original.read_text()=='preserve me' and not list(parent.iterdir())
        finally:
            if child.poll() is None: child.kill();child.wait()
            os.close(master)
    session(False);session(True)
print('scratch: actual Fish PTY, private mode, 0700/0600 ownership, XDG isolation, caller/original preservation, exit status and SIGTERM cleanup passed')
