#!/usr/bin/env python3
"""Same common-2.2.1 API, deterministic media and fault times for two revisions."""
import argparse, datetime as dt, hashlib, json, math, os, re, secrets, shutil, subprocess, sys, time
from pathlib import Path
ROOT=Path('/workspace/NoAr');sys.path.insert(0,str(ROOT/'qa/soak'))
from picture import PictureMonitor
from processing_matrix import Api

def dump(path,obj):path.write_text(json.dumps(obj,indent=2)+'\n')
def stop(p):
    if p is not None and p.poll() is None:
        p.terminate()
        try:p.wait(timeout=8)
        except subprocess.TimeoutExpired:p.kill();p.wait()

def scenario(args,protocol):
    root=args.work_root/args.label/protocol;root.mkdir(parents=True)
    for folder in ['db','media','playlists','public','logs','archive','captures']:(root/folder).mkdir()
    os.link(args.work_root/'fixture.mp4',root/'media/fixture.mp4')
    processes=[];handles=[];publisher=None;monitor=None
    result={'label':args.label,'protocol':protocol,'binary_sha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'settings':{'retry_seconds':120,'output':'640x360p25 HLS2s600segments','publisher':'ffmpeg testsrc2/sine h264ultrafastAAC','fault':'UDP video+audio to audio-only to video+audio' if protocol=='udp' else 'SRT publisher termination/restart','schedule_seconds':[5,105],'fault_seconds':[30,60] if protocol=='udp' else [30,50],'sample_seconds':125},'status':'RUNNING','actions':[]}
    def start(name,cmd,env=None):
        h=(root/(name+'.log')).open('a');handles.append(h)
        p=subprocess.Popen(cmd,stdout=h,stderr=subprocess.STDOUT,env=env);processes.append(p);return p
    def publish(mode):
        nonlocal publisher
        stop(publisher);publisher=None
        if mode=='off':return
        cmd=['ffmpeg','-nostdin','-v','warning','-re']
        if mode!='audio':cmd+=['-f','lavfi','-i','testsrc2=size=640x360:rate=25,drawbox=x=0:y=0:w=96:h=96:color=0x00ff00:t=fill']
        cmd+=['-f','lavfi','-i','sine=frequency=880:sample_rate=48000']
        if mode!='audio':cmd+=['-c:v','libx264','-preset','ultrafast','-tune','zerolatency','-g','25','-threads','2']
        cmd+=['-c:a','aac','-ac','2','-f','mpegts',('udp://127.0.0.1:5004?pkt_size=1316' if protocol=='udp' else 'srt://127.0.0.1:8890?streamid=publish:live/feed&latency=200000')]
        publisher=start('publisher-'+mode,cmd)
    try:
        if protocol=='srt':
            relay=start('mediamtx',[str(args.mediamtx),str(ROOT/'qa/fire/mediamtx.yml')]);time.sleep(1)
            if relay.poll() is not None:raise RuntimeError('dedicated relay failed')
        env={**os.environ,'DB':str(root/'db/noar.db'),'TZ':'UTC'}
        for key,folder in [('STORAGE','media'),('PLAYLISTS','playlists'),('PUBLIC','public'),('LOGS','logs')]:env['NOAR_SETUP_'+key]=str(root/folder)
        backend=start('backend',[str(args.binary),'--listen','127.0.0.1:18787','--log-to-console'],env)
        api=Api()
        for _ in range(150):
            if backend.poll() is not None:raise RuntimeError('backend exited before setup')
            try:initial=api.call('/api/setup');break
            except OSError:time.sleep(.2)
        else:raise RuntimeError('setup unavailable')
        assert initial['required'] and initial['settings']['storage']==str(root/'media')
        pwd=secrets.token_hex(20)
        api.call('/api/setup',{**initial['settings'],'username':'loss-operator','password':pwd,'mail':'loss@example.invalid','shared':False,'two_factor':False},'POST')
        api.token=api.call('/auth/login',{'username':'loss-operator','password':pwd},'POST')['access']
        channel=api.call('/api/channel/1');channel['timezone']='UTC';api.call('/api/channel/1',channel,'PATCH')
        config=api.call('/api/playout/config/1');output=next(v for v in api.call('/api/playout/outputs/1') if v['name']=='hls')
        config['output'].update(id=output['id'],mode='hls',width=640,height=360,fps=25,video_codec='libx264',audio_codec='aac',audio_bitrate=128,hls_playlist_name='loss',hls_segment_duration=2,hls_list_size=600,hls_variants=[])
        config['output']['video_options']['preset']='ultrafast';config['processing'].update(add_logo=False,mode='playlist');config['recording']['enable']=False
        config['storage']['filler']='fixture.mp4'
        epoch=math.ceil(time.time())+8;zero=dt.datetime.fromtimestamp(epoch,dt.timezone.utc)
        config['playlist'].update(day_start=zero.strftime('%H:%M:%S'),length='24:00:00',infinit=True)
        api.call('/api/playout/config/1',config,'PUT')
        for offset in [-1,0,1]:api.call('/api/playlist/1',{'channel':channel['name'],'date':str(zero.date()+dt.timedelta(days=offset)),'program':[{'source':str(root/'media/fixture.mp4'),'in':0,'out':60,'duration':60} for _ in range(24)]},'POST')
        url='udp://127.0.0.1:5004' if protocol=='udp' else 'srt://127.0.0.1:8890?streamid=read:live/feed&latency=200000'
        live=api.call('/api/schedule/1/lives',{'name':'Common '+protocol,'url':url},'POST')['path']
        event=api.call('/api/schedule/1',{'kind':'live','title':'Common fault '+protocol,'sources':[live],'start_date':str(zero.date()),'start_time':dt.datetime.fromtimestamp(epoch+5,dt.timezone.utc).strftime('%H:%M:%S'),'end_time':dt.datetime.fromtimestamp(epoch+105,dt.timezone.utc).strftime('%H:%M:%S'),'entry_mode':'interrupt','after_mode':'clock','retry_seconds':120},'POST')
        result['event']=event['id'];publish('av')
        while time.time()<epoch:time.sleep(.05)
        api.call('/api/control/1/process',{'command':'start'},'POST')
        monitor=PictureMonitor('http://127.0.0.1:18787/public/1/live/loss.m3u8',root,name='video-roi',video_filter='crop=120:60:240:140,fps=2,scale=160:90,format=gray');monitor.start()
        actions=[(30,'audio' if protocol=='udp' else 'off'),(60 if protocol=='udp' else 50,'av')]
        with (root/'samples.jsonl').open('w') as samples:
            while (elapsed:=time.time()-epoch)<125:
                while actions and elapsed>=actions[0][0]:
                    planned,mode=actions.pop(0);publish(mode);result['actions'].append({'planned':planned,'elapsed':time.time()-epoch,'mode':mode})
                playlist=root/'public/live/loss.m3u8'
                completed=[] if not playlist.exists() else [line for line in playlist.read_text().splitlines() if line.endswith('.ts')]
                for filename in completed:
                    f=root/'public/live'/filename;dest=root/'archive'/f.name
                    if f.exists() and not dest.exists():shutil.copy2(f,dest)
                health=api.call('/health');media=api.call('/api/control/1/media/current')
                samples.write(json.dumps({'elapsed':elapsed,'health':health,'media':media,'picture':monitor.sample()})+'\n');samples.flush()
                if backend.poll() is not None:raise RuntimeError('output process exited')
                time.sleep(.2)
        result['health']=api.call('/health');result['picture']=monitor.sample();result['status']='COMPLETED';dump(root/'run-result.json',result)
        api.call('/api/control/1/process',{'command':'stop'},'POST')
    except Exception as e:result.update(status='FAILED',error=repr(e))
    finally:
        if monitor:monitor.close()
        for p in reversed(processes):stop(p)
        for h in handles:h.close()
    # Decode all original segments in media order; exact timestamps are saved per frame.
    segments=sorted((root/'archive').glob('*.ts'),key=lambda p:int(re.search(r'(\d+)\.ts$',p.name).group(1)) if re.search(r'(\d+)\.ts$',p.name) else p.name)
    if segments:
        concat=root/'segments.txt';concat.write_text(''.join(f"file '{f}'\n" for f in segments))
        command=['ffmpeg','-nostdin','-v','warning','-f','concat','-safe','0','-i',str(concat),'-an','-vf','crop=120:60:240:140,fps=25,scale=80:40,format=gray','-f','framemd5',str(root/'video-roi.framemd5')]
        with (root/'decode.log').open('w') as log:subprocess.run(command,stdout=log,stderr=log,timeout=90,check=True)
        frames=[line.split(',') for line in (root/'video-roi.framemd5').read_text().splitlines() if line and not line.startswith('#')]
        holds=[];first=0
        for i in range(1,len(frames)+1):
            if i<len(frames) and frames[i][-1].strip()==frames[first][-1].strip():continue
            if i-first>=5:holds.append({'first_output_frame':first,'first_seconds':first/25,'frames':i-first,'seconds':(i-first)/25,'md5':frames[first][-1].strip()})
            first=i
        result['decoded']={'frames':len(frames),'longest_exact_hold_seconds':max([h['seconds'] for h in holds],default=0),'holds':holds}
    raw=(root/'backend.log').read_text();clean=re.sub('<[^>]+>','',raw)
    result['warnings']=[line for line in clean.splitlines() if re.search('pacing|out of sync|fora de sincronia|WARN|ERROR|jump|reset',line,re.I)]
    dump(root/'result.json',result);print(json.dumps(result),flush=True);return result

def main():
    p=argparse.ArgumentParser();p.add_argument('--binary',type=Path,required=True);p.add_argument('--mediamtx',type=Path,required=True);p.add_argument('--work-root',type=Path,required=True);p.add_argument('--label',choices=['main','branch'],required=True);p.add_argument('--protocols',nargs='+',choices=['udp','srt'],default=['udp','srt']);a=p.parse_args()
    if os.getenv('NOAR_SOAK')!='1':p.error('NOAR_SOAK=1 required for new owned disposable tests')
    a.binary=a.binary.resolve();a.mediamtx=a.mediamtx.resolve();a.work_root=a.work_root.resolve();a.work_root.mkdir(parents=True,exist_ok=True)
    if not (a.work_root/'fixture.mp4').exists():subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','testsrc2=size=640x360:rate=25,drawbox=x=0:y=0:w=96:h=96:color=blue:t=fill','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-t','60','-c:v','libx264','-preset','ultrafast','-threads','2','-g','25','-pix_fmt','yuv420p','-c:a','aac','-ac','2',str(a.work_root/'fixture.mp4')],check=True)
    results=[scenario(a,protocol) for protocol in a.protocols];dump(a.work_root/(a.label+'-summary.json'),results)
    raise SystemExit(0 if all(r['status']=='COMPLETED' for r in results) else 1)
if __name__=='__main__':main()
