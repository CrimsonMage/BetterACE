//! Bounded CPU texture expansion for the asset inspector.
use bace_dat::DatTexture;

pub(crate) fn pixels(texture:&DatTexture,palette:&[u32])->Result<Vec<[u8;4]>,String> {
    let n=texture.width as usize*texture.height as usize;
    let data=&texture.bytes;
    if matches!(texture.format,827611204|861165636|894720068) {return dxt(texture);}
    let stride=match texture.format {20|243=>3,21|22=>4,23|24|25|26|101=>2,28|41|50|244=>1,_=>return Err(format!("Texture {:08X}: unsupported pixel format {}",texture.id,texture.format))};
    if data.len()!=n*stride {return Err("Texture byte count does not match dimensions".into());}
    data.chunks_exact(stride).map(|p| Ok(match texture.format {
        20=>[p[2],p[1],p[0],255],243=>[p[0],p[1],p[2],255],
        21|22=>[p[2],p[1],p[0],if texture.format==22 {255}else{p[3]}],
        41|101=>{let index=if stride==1 {usize::from(p[0])} else {usize::from(u16::from_le_bytes([p[0],p[1]]))};argb(*palette.get(index).ok_or("Palette index out of bounds")?)},
        23=>rgb565(u16::from_le_bytes([p[0],p[1]])),
        24|25=>{let v=u16::from_le_bytes([p[0],p[1]]);let c=|shift| {let n=((v>>shift)&31_u16) as u8;(n<<3)|(n>>2)};[c(10),c(5),c(0),if texture.format==24||v&0x8000!=0 {255}else{0}]},
        26=>{let v=u16::from_le_bytes([p[0],p[1]]);[((v>>8)&15) as u8*17,((v>>4)&15) as u8*17,(v&15) as u8*17,((v>>12)&15) as u8*17]},
        _=>[p[0],p[0],p[0],255],
    })).collect()
}
pub(crate) fn argb(v:u32)->[u8;4] {[(v>>16) as u8,(v>>8) as u8,v as u8,(v>>24) as u8]}
fn rgb565(v:u16)->[u8;4] {
    let r=((v>>11)&31) as u8;let g=((v>>5)&63) as u8;let b=(v&31) as u8;
    [(r<<3)|(r>>2),(g<<2)|(g>>4),(b<<3)|(b>>2),255]
}
fn dxt(t:&DatTexture)->Result<Vec<[u8;4]>,String> {
    let width=t.width as usize;let height=t.height as usize;let stride=if t.format==827611204 {8}else{16};
    let bw=width.div_ceil(4);let bh=height.div_ceil(4);
    if t.bytes.len()!=bw*bh*stride {return Err("DXT byte count does not match dimensions".into());}
    let mut out=vec![[0;4];width*height];
    for (i,block) in t.bytes.chunks_exact(stride).enumerate() {
        let colors=&block[stride-8..];let c0=u16::from_le_bytes([colors[0],colors[1]]);let c1=u16::from_le_bytes([colors[2],colors[3]]);
        let mut table=[rgb565(c0),rgb565(c1),[0;4],[0;4]];
        let four=c0>c1||stride==16;
        for k in 0..3 {
            table[2][k]=if four {((2*u16::from(table[0][k])+u16::from(table[1][k]))/3) as u8} else {((u16::from(table[0][k])+u16::from(table[1][k]))/2) as u8};
            table[3][k]=if four {((u16::from(table[0][k])+2*u16::from(table[1][k]))/3) as u8}else{0};
        }
        table[2][3]=255;table[3][3]=if four {255}else{0};
        let indices=u32::from_le_bytes([colors[4],colors[5],colors[6],colors[7]]);
        let mut alpha=[0_u8;8];alpha[0]=block[0];alpha[1]=block[1];
        if t.format==894720068 {
            let steps=if alpha[0]>alpha[1] {7}else{5};
            for j in 1..steps {alpha[j+1]=(((steps-j) as u16*u16::from(alpha[0])+j as u16*u16::from(alpha[1]))/steps as u16) as u8;}
            if steps==5 {alpha[6]=0;alpha[7]=255;}
        }
        let alpha_bits=block[..stride.min(8)].iter().enumerate().fold(0_u64,|a,(j,b)|a|(u64::from(*b)<<(8*j)));
        for p in 0..16 {
            let x=(i%bw)*4+p%4;let y=(i/bw)*4+p/4;if x>=width||y>=height {continue;}
            let mut color=table[((indices>>(2*p))&3) as usize];
            if t.format==861165636 {color[3]=((alpha_bits>>(4*p))&15) as u8*17;}
            if t.format==894720068 {color[3]=alpha[((alpha_bits>>(16+3*p))&7) as usize];}
            out[y*width+x]=color;
        }
    }
    Ok(out)
}
