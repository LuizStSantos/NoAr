#!/usr/bin/env python3
"""Serial, disposable 1080p25 processing-bench comparison; no production target."""
import argparse, datetime as dt, hashlib, json, os, re, secrets, shutil, subprocess, time, urllib.request
from pathlib import Path
BASE='http://127.0.0.1:18787'
class Api:
    token=''
    def __init__(self): self.client=urllib.request.build_opener(urllib.request.ProxyHandler({}))
    def call(self,path,body=None,method=None):
        req=urllib.request.Request(BASE+path,data=None if body is None else json.dumps(body).encode(),method=method,
            headers={'Content-Type':'application/json',**({'Authorization':'Bearer '+self.token} if self.token else {})})
        with self.client.open(req,timeout=15) as r:
            raw=r.read().decode()
            try:return json.loads(raw)
            except ValueError:return raw

def resources(pid):
    stat=Path(f'/proc/{pid}/stat').read_text().split()
    return {'cpu_seconds':(int(stat[13])+int(stat[14]))/os.sysconf('SC_CLK_TCK'),
            'rss_bytes':int(stat[23])*os.sysconf('SC_PAGE_SIZE')}

def run(args,label,case,binary):
    root=args.work_root/label/case;root.mkdir(parents=True)
    for folder in ['db','media','playlists','public','logs']:(root/folder).mkdir()
    for asset in ['fixture.mp4','art.png']:
        os.link(args.work_root/asset,root/'media'/asset)
    env={**os.environ,'DB':str(root/'db/noar.db'),'TZ':'UTC'}
    for key,folder in [('STORAGE','media'),('PLAYLISTS','playlists'),('PUBLIC','public'),('LOGS','logs')]:env['NOAR_SETUP_'+key]=str(root/folder)
    result={'label':label,'case':case,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'profile':'dev unoptimized; processing-bench; no default features','output':'1920x1080p25 HLS x264 ultrafast, AAC128','warmup_seconds':15,'sample_seconds':args.seconds,'passed':False}
    with (root/'backend.log').open('w') as log:
        p=subprocess.Popen([str(binary),'--listen','127.0.0.1:18787','--log-to-console','--processing-bench-interval','5'],env=env,stdout=log,stderr=subprocess.STDOUT)
        api=Api()
        try:
            for _ in range(150):
                if p.poll() is not None:raise RuntimeError('backend exited at startup')
                try:initial=api.call('/api/setup');break
                except OSError:time.sleep(.2)
            else:raise RuntimeError('setup unavailable')
            assert initial['required'] and initial['settings']['storage']==str(root/'media')
            password=secrets.token_hex(20)
            api.call('/api/setup',{**initial['settings'],'username':'bench','password':password,'mail':'bench@example.invalid','shared':False,'two_factor':False},'POST')
            api.token=api.call('/auth/login',{'username':'bench','password':password},'POST')['access']
            ch=api.call('/api/channel/1');ch['timezone']='UTC';api.call('/api/channel/1',ch,'PATCH')
            config=api.call('/api/playout/config/1');output=next(v for v in api.call('/api/playout/outputs/1') if v['name']=='hls')
            config['output'].update(id=output['id'],mode='hls',width=1920,height=1080,fps=25,video_codec='libx264',audio_codec='aac',audio_bitrate=128,hls_playlist_name='bench',hls_segment_duration=2,hls_list_size=6,hls_variants=[])
            config['output']['video_options']['preset']='ultrafast'
            config['processing'].update(mode='playlist',add_logo=False,lshape='art.png',lshape_scale=75,lshape_corner='top-left')
            config['storage']['filler']='fixture.mp4';config['recording']['enable']=False
            now=dt.datetime.now(dt.timezone.utc)-dt.timedelta(seconds=1)
            config['playlist'].update(day_start=now.strftime('%H:%M:%S'),length='24:00:00',infinit=True)
            api.call('/api/playout/config/1',config,'PUT')
            for offset in [-1,0,1]:api.call('/api/playlist/1',{'channel':ch['name'],'date':str(now.date()+dt.timedelta(days=offset)),'program':[{'source':str(root/'media/fixture.mp4'),'in':0,'out':60,'duration':60} for _ in range(120)]},'POST')
            if label=='branch':
                for template in api.call('/api/graphics/1/templates'):
                    if template['always_on']:
                        template['always_on']=False;api.call('/api/graphics/1/templates/'+str(template['id']),template,'PUT')
            api.call('/api/control/1/process',{'command':'start'},'POST')
            deadline=time.monotonic()+30
            while not list((root/'public/live').glob('*.ts')):
                if time.monotonic()>deadline:raise RuntimeError('no actual HLS output')
                time.sleep(.2)
            count=int(case) if case.isdecimal() else (16 if case=='16-lshape-scroll' else 0)
            if label=='branch':
                for i in range(count):
                    x=2+(i%4)*24;y=3+(i//4)*18
                    template=api.call('/api/graphics/1/templates',{'name':f'GC {i+1}','group':'Bench','duration':0,'layers':[
                        {'id':'bar','kind':'box','x':x,'y':y,'width':22,'height':12,'fill':'#203050','opacity':.85},
                        {'id':'text','kind':'text','text':f'GC {i+1} - 1080p25','x':x+1,'y':y+1,'width':20,'height':10,'font_size':4,'fill':'#ffffff'}]},'POST')
                    api.call('/api/graphics/1/take',{'template_id':template['id']},'POST')
                if 'scroll' in case:
                    api.call('/api/graphics/1/templates',{'name':'Bench crawl','group':'Bench','duration':0,'always_on':True,'layers':[
                        {'id':'crawl','kind':'text','text':'PROCESSING BENCHMARK | DETERMINISTIC SCROLLING TEXT | 1080p25','x':2,'y':90,'width':96,'height':8,'font_size':4,'fill':'#ffffff','scroll':{'direction':'left','speed':80,'gap':80}}]},'POST')
            if 'lshape' in case:api.call('/api/control/1/lshape',{'action':'on'},'POST')
            time.sleep(15)
            before=resources(p.pid);start=time.monotonic();samples=[]
            while time.monotonic()-start<args.seconds:
                samples.append({'elapsed':time.monotonic()-start,**resources(p.pid),'health':api.call('/health')})
                time.sleep(1)
            after=resources(p.pid);elapsed=time.monotonic()-start
            result.update(elapsed=elapsed,process_cpu_percent=(after['cpu_seconds']-before['cpu_seconds'])/elapsed*100,
                rss_min=min(s['rss_bytes'] for s in samples),rss_max=max(s['rss_bytes'] for s in samples),
                healthy=all(s['health']['status']=='ok' for s in samples),samples=len(samples))
            if label=='branch':result['on_air']=api.call('/api/graphics/1/on-air')
            result['passed']=result['healthy'] and elapsed>=args.seconds
            (root/'resources.json').write_text(json.dumps(samples,indent=2)+'\n')
            api.call('/api/control/1/process',{'command':'stop'},'POST')
        except Exception as e:result['error']=str(e)
        finally:
            p.terminate()
            try:p.wait(timeout=12)
            except subprocess.TimeoutExpired:p.kill();p.wait()
    raw=(root/'backend.log').read_text()
    result['bench_reports']=raw.count('[CPU Bench]')
    result['frame_drops']=len(re.findall(r'dropp|pacing|out of sync|fora de sincronia',raw,re.I))
    result['bench_graphics_present']='graphics_blend' in raw
    result['passed'] &= result['bench_reports']>=3
    (root/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result),flush=True)
    return result

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--work-root',type=Path,required=True);parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--label',choices=['main','branch'],required=True);parser.add_argument('--seconds',type=int,default=60);parser.add_argument('--cases',nargs='+');args=parser.parse_args()
    if os.getenv('NOAR_SOAK')!='1' or args.seconds<30:parser.error('explicit disposable NOAR_SOAK=1 and >=30 measured seconds required')
    args.work_root=args.work_root.resolve();args.binary=args.binary.resolve();args.work_root.mkdir(exist_ok=True,parents=True)
    media=args.work_root/'fixture.mp4'
    if not media.exists():subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','testsrc2=size=1920x1080:rate=25','-f','lavfi','-i','sine=frequency=440:sample_rate=48000','-t','60','-c:v','libx264','-threads','2','-preset','ultrafast','-g','25','-pix_fmt','yuv420p','-c:a','aac','-ac','2',str(media)],check=True)
    if not (args.work_root/'art.png').exists():subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','color=c=0x183878:s=1920x1080','-frames:v','1','-threads','1',str(args.work_root/'art.png')],check=True)
    results=[run(args,args.label,case,args.binary) for case in (args.cases or (['0','lshape'] if args.label=='main' else ['0','1','4','16','lshape','scroll','16-lshape-scroll']))]
    (args.work_root/(args.label+'-summary.json')).write_text(json.dumps(results,indent=2)+'\n')
    raise SystemExit(0 if all(r['passed'] for r in results) else 1)
if __name__=='__main__':main()
