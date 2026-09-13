#!/usr/bin/env python3
"""Hermetic behavioral regressions for review fixes; no real process/service targets."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]

class ReviewRegressions(unittest.TestCase):
    def test_skill_install_uses_shipped_skills(self):
        with tempfile.TemporaryDirectory() as d:
            home = Path(d)
            (home / '.codex').mkdir()
            subprocess.run(['bash', str(ROOT / 'scripts/install-agent-skill.sh')], env={**os.environ, 'HOME': d}, check=True, capture_output=True)
            source = ROOT / 'plugins/axon/skills'
            expected = sorted(str(p.relative_to(source)) for p in source.rglob('SKILL.md'))
            actual = sorted(str(p.relative_to(home / '.codex/skills')) for p in (home / '.codex/skills').rglob('SKILL.md'))
            self.assertTrue(expected)
            self.assertEqual(actual, expected)

    def test_cleanup_keeps_old_active_process_unless_age_is_explicit(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); fake=root/'bin'; fake.mkdir()
            scripts={
                'pgrep': 'echo 99999999',
                'ps': "case \"$*\" in *stat=*) echo S;; *ppid=*) echo 42;; *comm=*) echo worker;; *etimes=*) echo 999999;; *rss=*) echo 1024;; *) echo '?';; esac",
                'pkill': 'echo unexpected-kill >> \"$KILL_LOG\"',
            }
            for name,body in scripts.items():
                path=fake/name;path.write_text('#!/bin/sh\n'+body+'\n');path.chmod(0o755)
            init=root/'init.sh'
            init.write_text("kill() { echo unexpected-kill >> \"$KILL_LOG\"; }\nmapfile() { local name=\"$2\" line i=0; while IFS= read -r line; do read -r \"${name}[$i]\" <<< \"$line\"; i=$((i+1)); done; }\n")
            env={**os.environ,'PATH':f'{fake}:{os.environ["PATH"]}','BASH_ENV':str(init),'KILL_LOG':str(root/'kills')}
            result=subprocess.run(['bash',str(ROOT/'scripts/cleanup-claude.sh'),'--kill'],env=env,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('KEEP',result.stdout)
            self.assertFalse((root/'kills').exists())
            result=subprocess.run(['bash',str(ROOT/'scripts/cleanup-claude.sh'),'--max-age','60'],env=env,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('explicit lifetime limit',result.stdout)
            self.assertFalse((root/'kills').exists())

    def test_cleanup_keeps_stopped_process_with_live_parent(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            fake = root / 'bin'
            fake.mkdir()
            scripts = {
                'pgrep': 'echo 99999999',
                'ps': "case \"$*\" in *stat=*) echo T;; *ppid=*) echo 42;; *comm=*) echo worker;; *etimes=*) echo 30;; *rss=*) echo 1024;; *) echo '?';; esac",
                'pkill': 'echo unexpected-kill >> "$KILL_LOG"',
            }
            for name, body in scripts.items():
                path = fake / name
                path.write_text('#!/bin/sh\n' + body + '\n')
                path.chmod(0o755)
            init = root / 'init.sh'
            init.write_text("kill() { echo unexpected-kill >> \"$KILL_LOG\"; }\nmapfile() { local name=\"$2\" line i=0; while IFS= read -r line; do read -r \"${name}[$i]\" <<< \"$line\"; i=$((i+1)); done; }\n")
            env = {**os.environ, 'PATH': f'{fake}:{os.environ["PATH"]}', 'BASH_ENV': str(init), 'KILL_LOG': str(root / 'kills')}
            result = subprocess.run(['bash', str(ROOT / 'scripts/cleanup-claude.sh'), '--kill'], env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('KEEP', result.stdout)
            self.assertFalse((root / 'kills').exists())

    def test_backup_assertion_rejects_leaks_in_either_stream(self):
        for stream in ['stdout', 'stderr']:
            with self.subTest(stream=stream), tempfile.TemporaryDirectory() as d:
                root = Path(d)
                (root / 'scripts').mkdir()
                for name in ['test-axon-backup.sh', 'axon-backup.sh']:
                    shutil.copy(ROOT / 'scripts' / name, root / 'scripts' / name)
                p = root / 'scripts/axon-backup.sh'
                lines = p.read_text().splitlines(True)
                lines.insert(1, "echo 'secret api key'" + (' >&2' if stream == 'stderr' else '') + '\n')
                p.write_text(''.join(lines))
                result = subprocess.run(['bash', str(root / 'scripts/test-axon-backup.sh')], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn('backup leaked a credential', result.stderr)

    def test_auto_tag_creates_annotated_tag_and_rejects_lightweight(self):
        workflow=(ROOT/'.github/workflows/auto-tag.yml').read_text()
        block=workflow.split('      - name: Create and push tag\n',1)[1].split('      - name:',1)[0]
        script='\n'.join(line[10:] for line in block.splitlines()[1:] if line.startswith('          '))
        with tempfile.TemporaryDirectory() as d:
            root=Path(d); remote=root/'remote.git'; checkout=root/'checkout'
            def git(*args):
                return subprocess.check_output(['git',*args],cwd=checkout if checkout.exists() else root,text=True,stderr=subprocess.DEVNULL).strip()
            git('init','--bare',str(remote)); git('clone',str(remote),str(checkout))
            git('config','user.name','Fixture');git('config','user.email','fixture@example.invalid')
            git('checkout','-b','main');(checkout/'fixture').write_text('fixture')
            git('add','.');git('commit','-m','fixture');git('push','origin','main')
            sha=git('rev-parse','HEAD')
            script=script.replace('${{ matrix.candidate_tag }}','v1.0.0').replace('${{ needs.plan.outputs.target_sha }}',sha)
            for _ in range(2):
                result=subprocess.run(['bash','-c',script],cwd=checkout,capture_output=True,text=True)
                self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(git('cat-file','-t','refs/tags/v1.0.0'),'tag')
            git('tag','v1.0.1')
            result=subprocess.run(['bash','-c',script.replace('v1.0.0','v1.0.1')],cwd=checkout,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('must be annotated',result.stderr)

    def test_incus_readiness_requires_running_binary_identity(self):
        source=(ROOT/'deploy/incus/bootstrap.sh').read_text()
        start=source.index('      set -eu\n      pid=')
        end=source.index("    ' sh \"$host_sha\"",start)
        command=source[start:end]
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            for name,body in [('systemctl','echo "${MOCK_PID:-123}"'),('sha256sum','echo "${MOCK_HASH:-expected}  fixture"'),('curl','echo checked >> "$MOCK_HEALTH"')]:
                p=root/name;p.write_text('#!/bin/sh\n'+body+'\n');p.chmod(0o755)
            for pid,digest,passes in [('123','expected',True),('123','old-binary',False),('0','expected',False)]:
                marker=root/'health';marker.unlink(missing_ok=True)
                env={**os.environ,'PATH':f'{root}:{os.environ["PATH"]}','MOCK_PID':pid,'MOCK_HASH':digest,'MOCK_HEALTH':str(marker)}
                result=subprocess.run(['sh','-c',command,'sh','expected'],env=env,capture_output=True,text=True)
                self.assertEqual(result.returncode==0,passes,result.stderr)
                self.assertEqual(marker.exists(),passes)

    def test_incus_bounded_probe_terminates_hanging_client(self):
        source = (ROOT / 'deploy/incus/bootstrap.sh').read_text()
        start = source.index('bounded_incus_probe() {')
        end = source.index('\n}\n\nsleep_before_retry()', start) + 3
        function = source[start:end]
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            incus = root / 'incus'
            incus.write_text('#!/bin/sh\nsleep 30\n')
            incus.chmod(0o755)
            timeout = root / 'timeout'
            timeout.write_text('''#!''' + sys.executable + '''
import os, signal, subprocess, sys
args=sys.argv[1:]
while args and args[0].startswith('--'): args.pop(0)
seconds=float(args.pop(0).removesuffix('s'))
process=subprocess.Popen(args,start_new_session=True)
try: process.wait(timeout=seconds)
except subprocess.TimeoutExpired:
 os.killpg(process.pid,signal.SIGKILL); process.wait(); sys.exit(124)
sys.exit(process.returncode)
''')
            timeout.chmod(0o755)
            script = function + '\nSECONDS=0\nbounded_incus_probe 1 5 info fixture\n'
            started = time.monotonic()
            result = subprocess.run(['bash', '-c', script], env={**os.environ, 'PATH': f'{root}:{os.environ["PATH"]}'}, capture_output=True, text=True, timeout=5)
            elapsed = time.monotonic() - started
            self.assertEqual(result.returncode, 124, result.stderr)
            self.assertLess(elapsed, 3)

    def test_incus_nested_health_requires_every_expected_service_exactly_once(self):
        source = (ROOT / 'deploy/incus/bootstrap.sh').read_text()
        start = source.index('nested_statuses_healthy() {')
        end = source.index('\n}\n\ncase "$MODE"', start) + 3
        function = source[start:end]
        cases = [
            ('axon-chrome:healthy\naxon-qdrant:healthy', True),
            ('axon-chrome:unhealthy\naxon-qdrant:healthy', False),
            ('axon-chrome:healthy', False),
            ('axon-chrome:healthy\naxon-qdrant:healthy\nunexpected:healthy', False),
        ]
        for statuses, expected in cases:
            with self.subTest(statuses=statuses):
                script = function + '\nnested_statuses_healthy "$1" axon-chrome axon-qdrant\n'
                result = subprocess.run(['bash', '-c', script, 'bash', statuses], capture_output=True, text=True)
                self.assertEqual(result.returncode == 0, expected, result.stderr)

    def test_incus_fresh_redeploy_and_disabled(self):
        for fresh, enabled in [(True, True), (False, True), (False, False)]:
            with self.subTest(fresh=fresh, enabled=enabled), tempfile.TemporaryDirectory() as d:
                root = Path(d); fake = root / 'bin'; fake.mkdir()
                envfile = root / 'env'; envfile.write_text('DOCKER_NETWORK=axon\nQDRANT_URL=http://wrong.invalid\n')
                binary = root / 'axon'; binary.write_text('fixture'); binary.chmod(0o755)
                state = root / 'state.json'; state.write_text(json.dumps({'exists': not fresh, 'env':False, 'restarted':False}))
                mock = fake / 'incus'
                mock.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
p=Path(os.environ['MOCK_STATE']); s=json.loads(p.read_text()); a=sys.argv[1:]; text=' '.join(a)
with open(os.environ['MOCK_LOG'],'a') as f: f.write(json.dumps(a)+'\\n')
code=0
if a[:2]==['profile','show']: code=0
elif a[:1]==['info']:
 if not s['exists']: code=1
 else: print('Status: RUNNING')
elif a[:1]==['launch']: s['exists']=True
elif a[:3]==['profile','device','get']: print('/data' if a[-1]=='path' else os.environ['MOCK_DATA'])
elif a[:2]==['file','push'] and a[-1]=='axon/data/.env': s['env']=True
elif '/etc/os-release' in text: print('ubuntu:26.04')
elif a[-2:]==['uname','-m']: print(os.uname().machine)
elif 'getconf GNU_LIBC_VERSION' in text: print('glibc 2.43')
elif 'sha256sum /usr/local/bin/axon.new' in text: print(os.environ['MOCK_SHA']+'  /usr/local/bin/axon.new')
elif '{{.Health}}' in text: print('axon-chrome:healthy')
elif 'endpoint=' in text:
 if not s['exists'] or not s['env'] or a[-1]!='http://selected.invalid': code=8
elif 'systemctl restart axon-native.service' in text: s['restarted']=True
elif 'http://127.0.0.1:8001/readyz' in text and not s['restarted']: code=9
p.write_text(json.dumps(s)); sys.exit(code)
'''); mock.write_text(mock.read_text().replace('#!/usr/bin/env python3', '#!' + sys.executable, 1)); mock.chmod(0o755)
                for name, body in [('getconf', 'echo "glibc 2.43"'), ('sleep','exit 77'), ('timeout', 'while [ "${1#--}" != "$1" ]; do shift; done; shift; exec "$@"')]:
                    p=fake/name;p.write_text('#!/bin/sh\n'+body+'\n');p.chmod(0o755)
                (root / 'data').mkdir(); (root/'data/.env').write_text('')
                env={**os.environ,'PATH':f'{fake}:{os.environ["PATH"]}', 'AXON_ENV_FILE':str(envfile), 'AXON_INCUS_BINARY':str(binary), 'AXON_EXTERNAL_QDRANT_URL':'http://selected.invalid', 'AXON_EXTERNAL_TEI_URL':'http://tei.invalid', 'AXON_INCUS_RUN_SERVER':str(enabled).lower(),'MOCK_STATE':str(state),'MOCK_LOG':str(root/'calls'),'MOCK_DATA':str(root/'data'),'MOCK_SHA':hashlib.sha256(binary.read_bytes()).hexdigest()}
                env.pop('container_data_path',None)
                result=subprocess.run(['bash',str(ROOT/'deploy/incus/bootstrap.sh')],env=env,capture_output=True,text=True,timeout=45)
                self.assertEqual(result.returncode,0,result.stdout+result.stderr)
                s=json.loads(state.read_text());self.assertEqual(s['restarted'],enabled)
                calls=[json.loads(line) for line in (root/'calls').read_text().splitlines()]
                env_index=next(i for i,a in enumerate(calls) if a[:2]==['file','push'] and a[-1]=='axon/data/.env')
                probe_index=next(i for i,a in enumerate(calls) if 'endpoint=' in ' '.join(a))
                self.assertLess(env_index,probe_index)

if __name__ == '__main__': unittest.main()
