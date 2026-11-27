import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
from tempfile import TemporaryDirectory
import unittest
from unittest.mock import patch
from tools import check_d3d12_regressions as controls
from tools.check_d3d12_regressions import validate,DIGEST,DEFAULT,LAUNCHER_CBV,launcher_cbv

class D3D12RegressionTests(unittest.TestCase):
    def test_adapter_and_exit_are_checked(self):
        line="d3d12-indirect PASS pci=10de:2704 list=59 words=64/64"
        self.assertTrue(validate("indirect","10de:2704",0,line)[0])
        self.assertFalse(validate("indirect","1002:1234",0,line)[0])
        self.assertFalse(validate("indirect","10de:2704",1,line)[0])
        self.assertFalse(validate("indirect","10de:2704",0,line+"\n"+line)[0])
        self.assertFalse(validate("indirect-scarlett","10de:2704",0,line)[0])
    def test_null_checkpoint_must_match(self):
        line="d3d12-null-usage PASS pci=10de:2704 words=64/64 recorded=64 valid=2"
        logs=[line]+[f"TuxBox d3d12_xs: substituted null compute pipeline for an untranslated shader ({n} so far)" for n in (1,2)]
        for interval,count in ((1,1),(2,63)):
            logs.append(f"TuxBox d3d12_xs: null usage interval recorded={count} display_bound=0 tracked=1 overflow=0 overflow_display_bound=0 compute={count} raster=0 interval={interval} trigger=recording_checkpoint")
            logs.append(f"TuxBox d3d12_xs: null usage rank kind=compute CS={DIGEST} recorded={count} display_bound=0 interval={interval} trigger=recording_checkpoint")
        output="\n".join(logs)
        self.assertTrue(validate("null-usage","10de:2704",0,output)[0])
        self.assertFalse(validate("null-usage","10de:2704",0,output.replace("compute=63","compute=64"))[0])
        self.assertFalse(validate("null-usage","10de:2704",0,line)[0])

    def test_rejects_fallback_and_contradictory_receipts(self):
        line="d3d12-indirect PASS pci=10de:2704 list=59 words=64/64"
        bad=["TuxBox d3d12_xs: substituted null compute pipeline for an untranslated shader (1 so far)",
             "TuxBox d3d12_xs: null usage rank kind=compute CS="+DIGEST,
             line.replace("10de:2704","1002:1234"),
             "d3d12-draw PASS pci=10de:2704 red=36 blue=220"]
        for other in bad:
            self.assertFalse(validate("indirect","10de:2704",0,line+"\n"+other)[0])
        self.assertFalse(validate("indirect","10de:2704",0,line+" words=1/64")[0])
        negative="d3d12-null-disabled PASS pci=10de:2704 rejected=1 valid=1"
        self.assertTrue(validate("null-disabled","10de:2704",0,negative)[0])
        self.assertFalse(validate("null-disabled","10de:2704",0,negative+"\n"+bad[0])[0])
    def test_buffer_import_requires_exact_backing_counts(self):
        line="d3d12-buffer-import PASS pci=10de:2704 pages=2 words=1536/1536 passes=2 aliases=2"
        records=[("pool_open",i) for i in range(1,4)]+[("pool_hit",i) for i in range(1,3)]+[("imported",i) for i in range(1,6)]
        logs=[line]+[f"TuxBox d3d12_xs: buffer backing path={p} count={i} source=place_virtual bytes=65536" for p,i in records]
        self.assertTrue(validate("buffer-import","10de:2704",0,"\n".join(logs))[0])
        self.assertFalse(validate("buffer-import","10de:2704",0,"\n".join(logs[:-1]))[0])
        self.assertFalse(validate("buffer-import","10de:2704",0,"\n".join(logs+[
            "TuxBox d3d12_xs: buffer backing path=device_standin count=1 source=place_virtual bytes=65536"]))[0])

    def test_late_commit_checks_both_imports_and_no_standin(self):
        line="d3d12-buffer-layout PASS pci=10de:2704 slot=133 words=512/512 VA-offsets=0,4"
        logs=[line]+[f"TuxBox d3d12_xs: buffer backing path={p} count={i} source=place_virtual_layout bytes={1024+4*(i-1)}"
                     for p in ("imported","commit_retry") for i in (1,2)]
        self.assertTrue(validate("buffer-layout","10de:2704",0,"\n".join(logs))[0])
        reuse=[f"TuxBox d3d12_xs: buffer backing path={p} count=1 source=place_virtual_layout bytes=65536"
               for p in ("page_open","page_hit","span_open","span_hit")]
        self.assertTrue(validate("buffer-layout","10de:2704",0,"\n".join(logs+reuse))[0])
        standin="TuxBox d3d12_xs: buffer backing path=device_standin count=1 source=place_virtual_layout bytes=1024"
        self.assertFalse(validate("buffer-layout","10de:2704",0,"\n".join(logs+reuse+[standin]))[0])
        self.assertFalse(validate("buffer-layout","10de:2704",0,"\n".join(logs[:-1]))[0])
        self.assertFalse(validate("buffer-layout","10de:2704",0,"\n".join(logs).replace("commit_retry","device_standin"))[0])
        negative="d3d12-buffer-import-reject PASS pci=10de:2704 rejected=1"
        record="TuxBox d3d12_xs: buffer backing path=strict_rejected count=1 source=place_virtual bytes=65536"
        self.assertTrue(validate("buffer-import-reject","10de:2704",0,negative+"\n"+record)[0])
        self.assertFalse(validate("buffer-import-reject","10de:2704",0,negative)[0])

    def test_remap_requires_retirement_and_reimport(self):
        line="d3d12-buffer-remap PASS pci=10de:2704 words=768/768 identical=1 replacement=1"
        records=[("pool_open",1),("pool_open",2),("pool_hit",1)]+[("imported",i) for i in range(1,4)]
        logs=[line]+[f"TuxBox d3d12_xs: buffer backing path={p} count={i} source=place_virtual_layout bytes=65536" for p,i in records]
        logs.append("TuxBox d3d12_xs: page remap replaced imported range retired stale GPU import")
        output="\n".join(logs)
        self.assertTrue(validate("buffer-remap","10de:2704",0,output)[0])
        self.assertFalse(validate("buffer-remap","10de:2704",0,"\n".join(logs[:-1]))[0])
        self.assertFalse(validate("buffer-remap","10de:2704",0,output.replace("pool_open count=2","pool_hit count=2"))[0])
        self.assertFalse(validate("buffer-remap","10de:2704",0,output+"\n"+logs[-1])[0])

    def test_unmap_released_must_keep_later_submits(self):
        line="d3d12-buffer-unmap PASS pci=1002:73df mode={} later-submits={}"
        self.assertTrue(validate("buffer-unmap-released","1002:73df",0,line.format("released","ok"))[0])
        self.assertFalse(validate("buffer-unmap-released","1002:73df",0,line.format("released","lost"))[0])
        self.assertFalse(validate("buffer-unmap-released","1002:73df",0,line.format("alive","ok"))[0])
        # Losing the device with the buffer alive is the host's behaviour, and is reported, not failed.
        self.assertTrue(validate("buffer-unmap-alive","1002:73df",0,line.format("alive","ok"))[0])
        self.assertTrue(validate("buffer-unmap-alive","1002:73df",0,line.format("alive","lost"))[0])
        self.assertFalse(validate("buffer-unmap-alive","1002:73df",0,line.format("released","ok"))[0])
        self.assertFalse(validate("buffer-unmap-alive","1002:73df",0,"d3d12-buffer-unmap PASS pci=1002:73df mode=alive")[0])

    def test_deferred_alias_refresh_requires_both_overlaps(self):
        receipt="d3d12-alias-deferred PASS pci=10de:2704 overlaps=2 old=verified refreshed=verified no_cpu_wait=1"
        line="placed texture refresh deferred until an overlapping write is published"
        output="\n".join([receipt,line,line,"alias deferred observed ms=0"])
        self.assertTrue(validate("alias-deferred","10de:2704",0,output)[0])
        self.assertFalse(validate("alias-deferred","10de:2704",0,output.replace("ms=0","ms=1999"))[0])
        self.assertFalse(validate("alias-deferred","10de:2704",0,output.replace(line,"unrelated",1))[0])
        self.assertFalse(validate("alias-deferred","10de:2704",0,output+"\nFailed to signal event")[0])

    def test_root_cbv_offsets_requires_every_16_byte_step(self):
        line="d3d12-root-cbv-offsets PASS pci=10de:2704 offsets=17/17 step=16 words=408/408"
        self.assertTrue(validate("root-cbv-offsets","10de:2704",0,line)[0])
        self.assertFalse(validate("root-cbv-offsets","10de:2704",0,line.replace("17/17","5/17"))[0])
        self.assertFalse(validate("root-cbv-offsets","10de:2704",1,"d3d12-root-cbv-window failed: root CBV offsets=5/17 wrong=16,32,48")[0])
        self.assertFalse(validate("root-cbv-window","10de:2704",0,line)[0])
        self.assertNotIn("root-cbv-offsets",DEFAULT)
        self.assertIn("root-cbv-offsets",LAUNCHER_CBV)

    def test_launcher_cbv_returns_what_the_launcher_sets(self):
        layer="/tuxbox/build/frosty-cbv-layer"
        with TemporaryDirectory() as temp, patch("project_tuxbox.console_cbv.console_cbv_layer",return_value=layer) as build:
            proton=Path(temp)
            # The guest's provider needs no match with the development build.
            with patch.dict(os.environ,{"VKD3D_CONFIG":"other"},clear=True):
                settings=launcher_cbv(proton,proton/"guest.exe")
            self.assertEqual(settings,{"VKD3D_CONFIG":"force_raw_va_cbv,other","VK_ADD_LAYER_PATH":layer,
                                       "VK_INSTANCE_LAYERS":"VK_LAYER_TUXBOX_console_cbv",
                                       "PRESSURE_VESSEL_FILESYSTEMS_RO":layer})
            build.assert_called_once()
            # A Proton build with the feature binds the offsets itself and takes no layer.
            (proton/"tuxbox-features").write_text("cbv_16_byte_alignment\n")
            with patch.dict(os.environ,{},clear=True):
                settings=launcher_cbv(proton,proton/"guest.exe")
            self.assertEqual(settings,{"VKD3D_CONFIG":"cbv_16_byte_alignment,force_raw_va_cbv",
                                       "VKD3D_DISABLE_EXTENSIONS":"VK_EXT_descriptor_buffer,VK_EXT_descriptor_heap"})
            build.assert_called_once()

    def run_controls(self,root,cases,settings):
        """Run the controls with every host tool replaced, returning the guest commands and the receipt."""
        for name in ("build/d3d12_xs.dll","build/d3d12-root-cbv-window.exe","dxc/dxcompiler.dll","dxc/dxil.dll",
                     "proton/files/bin/wine","proton/files/bin/wineserver","proton/version","prefix/user.reg",
                     "proton/files/lib/wine/vkd3d-proton/x86_64-windows/d3d12.dll",
                     "proton/files/lib/wine/vkd3d-proton/x86_64-windows/d3d12core.dll",
                     "proton/files/lib/wine/dxvk/x86_64-windows/dxgi.dll"):
            (root/name).parent.mkdir(parents=True,exist_ok=True)
            (root/name).write_text(name)
        commands=[]
        def guest(command,stdout,**_):
            commands.append(command)
            window="--offsets" not in command
            stdout.write(b"d3d12-root-cbv-window PASS pci=10de:2704 words=120/120 window=65536 offsets=0,256,65280,65536,65792\n"
                         if window else b"d3d12-root-cbv-offsets PASS pci=10de:2704 offsets=17/17 step=16 words=408/408\n")
            for i,n in enumerate((65536,131072,131072,65536,131072) if window else (),1):
                stdout.write(f"TuxBox d3d12_xs: buffer backing path=imported count={i} source=view bytes={n}\n".encode())
            return subprocess.CompletedProcess(command,0)
        arguments=["--adapter","10de:2704","--work",str(root/"work"),"--output",str(root/"out.json")]
        for name,value in (("build-dir","build"),("proton","proton"),("prefix","prefix"),("compiler-dir","dxc")):
            arguments+=["--"+name,str(root/value)]
        for case in cases:
            arguments+=["--case",case]
        with patch.object(controls.shutil,"which",return_value="/tool"), \
             patch.object(controls.subprocess,"run",side_effect=guest), \
             patch.object(controls,"launcher_cbv",side_effect=settings) as launcher, \
             contextlib.redirect_stdout(io.StringIO()) as printed:
            code=controls.main(arguments)
        return code,commands,json.loads((root/"out.json").read_text()),launcher,printed.getvalue()

    def test_root_cbv_offsets_runs_with_the_launcher_settings(self):
        settings={"VKD3D_CONFIG":"force_raw_va_cbv","VK_ADD_LAYER_PATH":"/tuxbox/build/frosty-cbv-layer",
                  "VK_INSTANCE_LAYERS":"VK_LAYER_TUXBOX_console_cbv"}
        with TemporaryDirectory() as temp:
            root=Path(temp)
            code,commands,report,launcher,_=self.run_controls(root,("root-cbv-window","root-cbv-offsets"),[settings])
            self.assertEqual(code,0)
            # Cases run side by side, so either guest can start first.
            window,offsets=sorted(commands,key=lambda command:"--offsets" in command)
            for key,value in settings.items():
                at=offsets.index(key)
                self.assertEqual(offsets[at-1:at+2],["--setenv",key,value])
                # The other controls keep running against vkd3d-proton's defaults.
                self.assertNotIn(key,window)
            proton,executable=launcher.call_args.args
            self.assertEqual(proton,(root/"proton").resolve())
            self.assertEqual(executable.name,"d3d12-root-cbv-window.exe")
            first,second=report["cases"]
            self.assertNotIn("launcher_cbv",first)
            self.assertTrue(second["passed"])
            self.assertEqual(second["launcher_cbv"],{"VKD3D_CONFIG":"force_raw_va_cbv",
                             "VK_INSTANCE_LAYERS":"VK_LAYER_TUXBOX_console_cbv","VKD3D_DISABLE_EXTENSIONS":""})
            # The receipt names the route without the path of the checkout.
            self.assertNotIn("frosty-cbv-layer",(root/"out.json").read_text())

    def test_root_cbv_offsets_fails_when_the_settings_cannot_be_made(self):
        with TemporaryDirectory() as temp:
            root=Path(temp)
            failed=subprocess.CalledProcessError(1,["build_frosty_cbv_layer.sh"])
            code,commands,report,_,printed=self.run_controls(root,("root-cbv-offsets",),[failed])
            # The guest is not run with other settings, where AMD would pass and say nothing.
            self.assertEqual((code,commands),(1,[]))
            self.assertEqual((report["cases"][0]["passed"],report["cases"][0]["exit"]),(False,1))
            self.assertIn("root-cbv-offsets FAIL constant-buffer setup failed",printed)
            self.assertIn("constant-buffer setup failed",(root/"work/root-cbv-offsets.log").read_text())

    def test_root_cbv_window_requires_full_imports(self):
        line="d3d12-root-cbv-window PASS pci=10de:2704 words=120/120 window=65536 offsets=0,256,65280,65536,65792"
        records=[f"TuxBox d3d12_xs: buffer backing path=imported count={i} source=view bytes={n}" for i,n in enumerate((65536,131072,131072,65536,131072),1)]
        output="\n".join([line]+records)
        self.assertTrue(validate("root-cbv-window","10de:2704",0,output)[0])
        self.assertFalse(validate("root-cbv-window","10de:2704",0,output.replace("131072","65536"))[0])
        self.assertFalse(validate("root-cbv-window","10de:2704",0,output+"\nbound with a short range")[0])
        self.assertFalse(validate("root-cbv-window","10de:2704",0,output+"\nsubstituted null compute pipeline")[0])
