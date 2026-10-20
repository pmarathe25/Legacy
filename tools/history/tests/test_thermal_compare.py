import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0,str(Path(__file__).parents[1]))
spec=importlib.util.spec_from_file_location('comparison',Path(__file__).parents[1]/'compare.py')
comparison=importlib.util.module_from_spec(spec);spec.loader.exec_module(comparison)

class ThermalComparisonTests(unittest.TestCase):
    def test_hottest_cpu_sensor_is_used_and_other_sensors_are_ignored(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder)
            for i,(kind,temp) in enumerate([('pch_skylake',99000),('x86_pkg_temp',68000),('cpu-thermal',74000)]):
                zone=root/f'thermal_zone{i}';zone.mkdir()
                (zone/'type').write_text(kind);(zone/'temp').write_text(str(temp))
            self.assertEqual(comparison.cpu_temperature(root),74)
        with tempfile.TemporaryDirectory() as folder:
            with self.assertRaisesRegex(RuntimeError,'unavailable'):comparison.cpu_temperature(Path(folder))

    def test_cooling_is_completed_before_timing_and_reports_its_separate_duration(self):
        with patch.object(comparison,'cpu_temperature',side_effect=[82,76,70]), patch.object(comparison.time,'monotonic',side_effect=[0,1,2,3]), patch.object(comparison.time,'sleep') as sleep:
            self.assertEqual(comparison.cool_before_run(72),(70,3))
            self.assertEqual(sleep.call_count,2)
        self.assertEqual(comparison.cool_before_run(None),(None,0))

    def test_hot_machine_timeout_does_not_start_a_replay(self):
        with patch.object(comparison,'cpu_temperature',return_value=90), patch.object(comparison.time,'monotonic',side_effect=[0,601]):
            with self.assertRaisesRegex(RuntimeError,'no timing run started'):comparison.cool_before_run(72)

    def test_thermal_abort_reaps_the_entire_measured_process_group(self):
        processes=[];spawn=subprocess.Popen
        def record(*args,**kwargs):
            child=spawn(*args,**kwargs);processes.append(child);return child
        with tempfile.TemporaryFile('w') as log, patch.object(comparison.subprocess,'Popen',side_effect=record), patch.object(comparison,'cpu_temperature',side_effect=[70,95]):
            with self.assertRaisesRegex(RuntimeError,'timing discarded'):
                comparison.run_measured([sys.executable,'-c','import time;time.sleep(30)'],dict(os.environ),log,88)
        self.assertEqual(len(processes),1)
        self.assertIsNotNone(processes[0].poll())

    def test_hot_start_does_not_spawn_a_replay(self):
        with patch.object(comparison.subprocess,'Popen') as spawn, patch.object(comparison,'cpu_temperature',return_value=95):
            with self.assertRaisesRegex(RuntimeError,'no replay started'):
                comparison.run_measured(['unused'],dict(os.environ),None,88)
            spawn.assert_not_called()

    def test_a_fast_completed_replay_still_checks_its_final_temperature(self):
        child=Mock(returncode=0,pid=123)
        child.poll.return_value=0
        with patch.object(comparison.subprocess,'Popen',return_value=child), patch.object(comparison.os,'killpg'), patch.object(comparison,'cpu_temperature',side_effect=[70,95]):
            with self.assertRaisesRegex(RuntimeError,'completed but timing discarded'):
                comparison.run_measured(['unused'],dict(os.environ),None,88)

    @unittest.skipUnless(hasattr(os, 'fork'), 'requires POSIX process groups')
    def test_failed_wrapper_does_not_leave_a_running_descendant(self):
        with tempfile.TemporaryDirectory() as folder, tempfile.TemporaryFile('w') as log:
            pid_file=Path(folder)/'child.pid'
            program="""import os,sys,time
child=os.fork()
if child == 0:
    time.sleep(30)
    sys.exit(0)
open(sys.argv[1], 'w').write(str(child))
sys.exit(1)
"""
            with patch.object(comparison,'cpu_temperature',return_value=70):
                with self.assertRaises(subprocess.CalledProcessError):
                    comparison.run_measured([sys.executable,'-c',program,str(pid_file)],dict(os.environ),log,88)
            child=int(pid_file.read_text())
            # A reparented zombie can remain briefly; it no longer consumes CPU.
            for _ in range(100):
                status=Path(f'/proc/{child}/stat')
                if not status.exists() or status.read_text().split()[2] == 'Z':
                    break
                time.sleep(0.01)
            else:
                os.kill(child,9)
                self.fail('failed timing wrapper left its measured child running')
