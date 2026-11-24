import sys
from PIL import Image, ImageFilter
import numpy as np
from collections import deque
S=sys.argv[1]; im=np.asarray(Image.open('assets/marke/markenpraesentation.png').convert('RGB')).astype(float)/255
def gross(a,schwelle,minimal):
    m=a>schwelle; h,w=m.shape; seen=np.zeros_like(m); keep=np.zeros_like(m)
    for y in range(h):
        for x in range(w):
            if m[y,x] and not seen[y,x]:
                q=deque([(y,x)]); seen[y,x]=1; c=[]
                while q:
                    i,j=q.popleft(); c.append((i,j))
                    for di in(-1,0,1):
                        for dj in(-1,0,1):
                            p,r=i+di,j+dj
                            if 0<=p<h and 0<=r<w and m[p,r] and not seen[p,r]: seen[p,r]=1; q.append((p,r))
                if len(c)>=minimal:
                    for i,j in c: keep[i,j]=1
    k=np.asarray(Image.fromarray((keep*255).astype('uint8')).filter(ImageFilter.MaxFilter(3)))>0
    return np.where(k,a,0)
def frei(box,hell,unten,oben,minimal):
    l,t,r,b=box; c=im[t:b,l:r]
    if hell: bg=np.array([.96,.96,1.0]); d=(bg-c).max(axis=2)
    else: bg=np.median(c.reshape(-1,3)[np.argsort(c.max(axis=2).ravel())[:400]],axis=0); d=(c-bg).max(axis=2)
    a=np.clip((d-unten)/(oben-unten),0,1); a=gross(a,.12,minimal); a3=a[...,None]
    rgb=np.where(a3>0,np.clip((c-bg*(1-a3))/np.maximum(a3,.02),0,1),0)
    o=Image.fromarray((np.dstack([rgb,a])*255).round().astype('uint8'),'RGBA'); return o.crop(o.getbbox())
d=frei((1003,628,1256,878),False,.30,.50,60); h=frei((412,638,630,858),True,.16,.34,60); m=frei((714,640,930,858),False,.35,.70,60)
m=Image.merge('RGBA',[Image.new('L',m.size,255)]*3+[m.split()[3]])
for n,x in (('dunkel',d),('hell',h),('mono',m)):
    k=max(x.size); q=Image.new('RGBA',(k,k),(0,0,0,0)); q.paste(x,((k-x.width)//2,(k-x.height)//2)); q.resize((192,192),Image.LANCZOS).save(f'{S}/logo-{n}.png',optimize=True)
w=Image.new('RGBA',(930,330),(13,17,23,255)); w.paste((245,246,248,255),(310,0,620,330)); w.paste((0,0,0,255),(620,0,930,330))
for i,n in enumerate(('dunkel','hell','mono')):
    x=Image.open(f'{S}/logo-{n}.png'); w.paste(x,(i*310+10,10),x)
    for k,dx in ((28,0),(36,50),(56,110)): t=x.resize((k,k),Image.LANCZOS); w.paste(t,(i*310+20+dx,240),t)
w.save(S+'/logo-vorschau.png')
