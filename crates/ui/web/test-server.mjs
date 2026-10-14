import http from 'node:http';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
const root=path.resolve('dist');
const port=Number(process.env.PORT||4173);
http.createServer(async(req,res)=>{
  const relative=new URL(req.url,'http://localhost').pathname.slice(1)||'index.html';
  const file=path.resolve(root,relative);
  if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}
  try {const data=await readFile(file);res.setHeader('Content-Type',file.endsWith('.js')?'text/javascript':file.endsWith('.css')?'text/css':file.endsWith('.html')?'text/html':file.endsWith('.ttf')?'font/ttf':'application/octet-stream');res.end(data);}
  catch{res.writeHead(404).end();}
}).listen(port,'127.0.0.1',()=>console.log(`Preview server: ${port}`));
