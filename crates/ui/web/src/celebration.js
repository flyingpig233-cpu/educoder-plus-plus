let frame = 0, timer = 0, lastId = null;
export function stopCelebration() {
  cancelAnimationFrame(frame); clearTimeout(timer);
  document.getElementById('ac-celebration').hidden = true;
  document.getElementById('confetti-canvas').getContext('2d').clearRect(0,0,innerWidth*2,innerHeight*2);
}
export function startCelebration(message) {
  if(message.id===lastId)return;
  lastId=message.id;stopCelebration();
  const overlay=document.getElementById('ac-celebration'), canvas=document.getElementById('confetti-canvas');
  const reduce=message.reduce_motion||matchMedia('(prefers-reduced-motion: reduce)').matches;
  const left=message.left??0, top=message.top??0, width=message.width??innerWidth, height=message.height??innerHeight;
  overlay.style.setProperty('--ac-left',`${width/2-left}px`);overlay.style.setProperty('--ac-top',`${height/2-top}px`);
  overlay.hidden=false;
  const ratio=Math.min(devicePixelRatio,2),ctx=canvas.getContext('2d');
  canvas.width=innerWidth*ratio;canvas.height=innerHeight*ratio;ctx.scale(ratio,ratio);
  const start=performance.now(),particles=(message.particles??[]).slice(0,120);
  const draw=now=>{
    const phase=Math.min(1,(now-start)/5000);
    ctx.clearRect(0,0,innerWidth,innerHeight);
    for(const p of particles){
      if(phase<p.delay)continue;
      const x=p.x*width+p.drift*phase*60-left, y=(phase-p.delay)*p.speed*(height+60)-40-top;
      if(y>innerHeight+30||y< -30)continue;
      ctx.save();ctx.translate(x+p.size/2,y+p.size*.275);ctx.rotate(p.spin*phase*Math.PI/180);
      ctx.fillStyle=p.color;ctx.fillRect(-p.size/2,-p.size*.275,p.size,p.size*.55);ctx.restore();
    }
    if(phase<1)frame=requestAnimationFrame(draw);
  };
  if(!reduce)frame=requestAnimationFrame(draw);
  timer=setTimeout(stopCelebration,reduce?1500:5000);
}
