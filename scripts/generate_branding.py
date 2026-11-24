"""Rebuild original app/tray artwork and DMG backdrop (Pillow 11+; macOS fonts for backdrop)."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'assets'/'branding'
OUT.mkdir(parents=True,exist_ok=True)
# 1024-point master, transparent outside Apple-style rounded tile.
im=Image.new('RGBA',(1024,1024))
mask=Image.new('L',im.size);ImageDraw.Draw(mask).rounded_rectangle((64,64,960,960),radius=204,fill=255)
bg=Image.new('RGBA',im.size);d=ImageDraw.Draw(bg)
for y in range(1024):
    t=y/1024
    d.line((0,y,1024,y),fill=(round(53-31*t),round(119-51*t),round(104-48*t),255))
bg.putalpha(mask);im.alpha_composite(bg)
d=ImageDraw.Draw(im)
d.rounded_rectangle((85,85,939,939),radius=184,outline=(155,214,190,80),width=3)
# Back card and front image; white/cream motif with simple mountainous silhouette.
d.rounded_rectangle((322,216,787,661),radius=60,fill='#76B8A6')
d.rounded_rectangle((232,309,704,780),radius=65,fill='#F2F5ED')
d.rounded_rectangle((267,344,669,745),radius=37,fill='#D3E7DB')
d.ellipse((526,389,608,471),fill='#E5B874')
# clip mountains into the image
scene=Image.new('RGBA',im.size);sd=ImageDraw.Draw(scene)
sd.polygon([(267,638),(401,470),(522,600),(594,523),(669,613),(669,745),(267,745)],fill='#639B8B')
sd.polygon([(267,691),(393,566),(506,654),(589,589),(669,664),(669,745),(267,745)],fill='#245E53')
clip=Image.new('L',im.size);ImageDraw.Draw(clip).rounded_rectangle((267,344,669,745),radius=37,fill=255)
from PIL import ImageChops
scene.putalpha(ImageChops.multiply(scene.getchannel('A'),clip));im.alpha_composite(scene)
im.save(OUT/'app.png');im.save(OUT/'app.ico',sizes=[(16,16),(24,24),(32,32),(48,48),(64,64),(128,128),(256,256)])
im.save(OUT/'app.icns')
# Single-color template for macOS menu bar; transparent RGBA and colored Windows counterpart.
for filename,color in [('tray-template.png','#000000'),('tray-color.png','#438E76')]:
    t=Image.new('RGBA',(96,96));d=ImageDraw.Draw(t)
    d.rounded_rectangle((16,23,77,82),radius=11,outline=color,width=7)
    d.line([(29,23),(29,14),(84,14),(84,66),(77,66)],fill=color,width=6,joint='curve')
    d.line([(24,65),(40,46),(53,59),(67,44),(74,54)],fill=color,width=6,joint='curve')
    d.ellipse((55,32,65,42),fill=color)
    t.resize((24,24),Image.Resampling.LANCZOS).save(OUT/filename)
# Actual Finder background, logical 640x420; no baked-in fake app icons.
font_path='/System/Library/Fonts/Avenir Next.ttc'
def font(n):return ImageFont.truetype(font_path,n)
w,h=640,420
im=Image.new('RGB',(w,h));d=ImageDraw.Draw(im)
for y in range(h):
    t=y/h;d.line((0,y,w,y),fill=(int(244-8*t),int(248-7*t),int(245-5*t)))
d.rounded_rectangle((27,25,613,395),radius=22,outline='#D1DED6',width=1)
d.text((320,53),'Yuque Asset Desk',fill='#224D42',font=font(26),anchor='mm')
d.text((320,84),'Install on your Mac',fill='#667B73',font=font(14),anchor='mm')
# Finder icons at (165,205) and (475,205), arrow between.
d.line((279,205,361,205),fill='#579783',width=3)
d.line([(349,193),(362,205),(349,217)],fill='#579783',width=3,joint='curve')
d.text((320,316),'Drag the app to Applications',fill='#2E6959',font=font(19),anchor='mm')
d.text((320,345),'Then open it from your Applications folder.',fill='#667B73',font=font(13),anchor='mm')
d.text((320,372),'Apple Silicon  •  macOS 15+',fill='#788B83',font=font(12),anchor='mm')
im.save(OUT/'dmg-background.png')
