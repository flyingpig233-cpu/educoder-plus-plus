import * as monaco from 'monaco-editor/editor/editor.api.js';
import 'monaco-editor/languages/definitions/cpp/register.js';
import 'monaco-editor/languages/definitions/rust/register.js';
import 'monaco-editor/languages/definitions/python/register.js';
import 'monaco-editor/languages/definitions/java/register.js';
import 'monaco-editor/languages/definitions/javascript/register.js';
import 'monaco-editor/languages/definitions/sql/register.js';
import 'monaco-editor/languages/definitions/shell/register.js';
import 'monaco-editor/editor/contrib/find/browser/findController.js';
import 'monaco-editor/editor/contrib/folding/browser/folding.js';
import 'monaco-editor/editor/contrib/bracketMatching/browser/bracketMatching.js';
import 'monaco-editor/editor/contrib/hover/browser/hoverContribution.js';
import 'monaco-editor/editor/contrib/comment/browser/comment.js';
import 'monaco-editor/features/contextmenu/register.js';
import 'monaco-editor/features/clipboard/register.js';
import { normalizeTaskMarkdown } from './markdown.js';
import { startCelebration, stopCelebration } from './celebration.js';
import { marked } from 'marked';
import DOMPurify from 'dompurify';
import './style.css';

self.MonacoEnvironment = { getWorker: () => new Worker(new URL('editor.worker.js', location.href), { name: 'educoder-editor' }) };
const $ = id => document.getElementById(id);
const pending = new Map();
const models = new Map();
const state = { game: '', task: null, base: new Map(), files: [], busy: false, connected: false, samples: [], sampleIndex: 0, sequence: 0 };
let draftTimer, draftQueue = Promise.resolve(), draftError = false, activeTab = 'local';
let answerData = null, answerGame = '', answerSequence = 0, answerLoading = false, answerImporting = false;
let syncTimer, syncPromise = null, syncFailed = false, resultTimer, resultAttempts = 0;
let celebrationArmed=false;
function animateContent(node) {
  if(matchMedia('(prefers-reduced-motion: reduce)').matches||document.documentElement.dataset.reducedMotion==='true')return;
  node.getAnimations().forEach(animation=>animation.cancel());
  node.animate([{opacity:.6,transform:'translateY(4px)'},{opacity:1,transform:'translateY(0)'}],{duration:180,easing:'cubic-bezier(.2,.7,.2,1)'});
}
const editor = monaco.editor.create($('editor'), { theme:'vs-dark', automaticLayout:true, fontFamily:'Consolas, monospace', fontSize:14, lineHeight:22, minimap:{enabled:false}, scrollBeyondLastLine:false, smoothScrolling:true, padding:{top:14}, readOnly:true, ariaLabel:'代码编辑器', tabSize:4, insertSpaces:true, fixedOverflowWidgets:true });
const language = path => ({rs:'rust',py:'python',c:'cpp',cpp:'cpp',cc:'cpp',cxx:'cpp',h:'cpp',java:'java',js:'javascript',mjs:'javascript',sql:'sql',sh:'shell'})[path.split('.').pop()?.toLowerCase()] || 'plaintext';

function notify(action, payload = {}) { window.ipc?.postMessage(JSON.stringify({ action, context:state.game, payload })); }
function request(action, payload = {}) {
  if (!window.ipc) return Promise.reject(new Error('桌面连接不可用'));
  const id = `${Date.now()}-${++state.sequence}`;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error('操作尚未完成，请刷新状态后确认结果。')); }, 180000);
    pending.set(id, { resolve, reject, timer });
    window.ipc.postMessage(JSON.stringify({ id, action, context:state.game, payload }));
  });
}
function error(message = '') { $('error').textContent = message; $('error').hidden = !message; }
function notice(message = '') { $('notice').textContent = message; $('notice').hidden = !message; }
function refreshLoading(id, loading) { $(id).setAttribute('aria-busy', String(loading)); }
function options(select, rows, value) {
  select.replaceChildren(...rows.map(row => { const option=document.createElement('option'); option.value=row.value; option.textContent=row.text; return option; }));
  if (value != null) select.value = value;
}
function allFiles() { return state.files.filter(f => !f.read_only).map(f => ({ path:f.path, content:models.get(`${state.game}:${f.path}`)?.getValue() ?? f.content })); }
function hasChanges() { return allFiles().some(f => state.base.get(f.path) !== f.content); }
function updateButtons() {
  const writable = !!state.task && state.files.some(f => !f.read_only);
  $('submit').disabled = state.busy || !!syncPromise || syncFailed || !state.connected || !state.task?.can_submit || !writable;
  $('sync-status').textContent = syncPromise?'正在同步到头歌…':syncFailed?'同步失败 · 草稿已保留':!state.connected?'离线 · 草稿已保留':hasChanges()?'等待自动同步':'已同步到头歌';
  $('run').disabled = state.busy || !writable;
  $('files').disabled = state.files.length === 0;
  $('games').disabled = state.busy;
  $('back').disabled = state.busy;
  $('refresh-task').disabled = state.busy || !state.task || !state.connected;
  $('refresh-result').disabled = state.busy || !state.task || !state.connected;
  $('enter').disabled = state.busy || !state.connected;
  $('answers').disabled = !state.task || state.busy;
  updateAnswerButtons();
  $('dirty').textContent = hasChanges() ? (draftError ? '草稿保存失败' : '本地草稿') : '';
}
function selectFile(path) {
  const file=state.files.find(f=>f.path===path); if(!file)return;
  const key=`${state.game}:${path}`;
  let model=models.get(key);
  if(!model){ model=monaco.editor.createModel(file.content, language(path), monaco.Uri.parse(`inmemory://educoder/${encodeURIComponent(state.game)}/${encodeURIComponent(path)}`)); models.set(key,model); }
  editor.setModel(model); editor.updateOptions({readOnly:!!file.read_only}); $('files').value=path; updateButtons();
}

function resetAnswers() {
  answerSequence++; answerData=null; answerGame=''; answerLoading=false;
  $('answer-dialog').close(); $('answer-code').value='';
}
function selectedAnswer() { return answerData?.files[Number($('answer-files').value)]; }
function updateAnswerButtons() {
  const file=selectedAnswer(), target=state.files.find(f=>f.path===$('answer-target').value&&!f.read_only);
  const active=!!file&&answerGame===state.game&&!answerLoading&&!answerImporting;
  $('copy-answer').disabled=!active;
  $('reload-answer').disabled=answerLoading||answerImporting||!state.task;
  refreshLoading('reload-answer',answerLoading);
  const sameType=file&&target&&file.path.split('.').pop().toLowerCase()===target.path.split('.').pop().toLowerCase();
  $('import-answer').disabled=!active||state.busy||!sameType;
  $('insert-answer').disabled=!active||state.busy||!target||$('answer-code').selectionStart===$('answer-code').selectionEnd;
  $('answer-target').disabled=answerImporting;
  $('answer-files').disabled=answerImporting||!active;
}
function showAnswerFile() {
  $('answer-code').value=selectedAnswer()?.content??'';
  const file=selectedAnswer(), writable=state.files.filter(f=>!f.read_only);
  const basename=path=>path.split('/').pop();
  const exact=writable.find(f=>file&&basename(f.path)===basename(file.path));
  options($('answer-target'),writable.map(f=>({value:f.path,text:f.path})),exact?.path??$('files').value);
  updateAnswerButtons();
}
async function loadAnswers(refresh=false) {
  if(!state.task||answerLoading)return;
  const game=state.game, sequence=++answerSequence;
  answerLoading=true;answerData=null;answerGame=game;
  $('answer-dialog').showModal();$('answer-title').textContent=`题解 · ${state.task.title}`;
  $('answer-status').textContent='正在查找答案…';$('answer-code').value='';options($('answer-files'),[]);updateAnswerButtons();
  try {
    const result=await request('answers',{refresh});
    if(sequence!==answerSequence||game!==state.game)return;
    answerData=result.answer;
    const status=({public_samples_passed:'公开样例通过',statement_examples_passed:'题面推导测试通过'})[answerData?.status]??answerData?.status??'';
    $('answer-status').textContent=answerData?`${status||"已找到题解"}${result.stale?' · 更新失败，显示旧缓存':result.cached?' · 本地缓存':''}`:'未找到对应答案';
    options($('answer-files'),(answerData?.files??[]).map((f,i)=>({value:String(i),text:f.path})),'0');showAnswerFile();
  } catch(e) { if(sequence===answerSequence)$('answer-status').textContent=`答案查找失败：${e.message}，可点击重新查找。`; }
  finally {if(sequence===answerSequence){answerLoading=false;updateAnswerButtons();}}
}
async function copyAnswer() {
  const file=selectedAnswer();if(!file||answerGame!==state.game)return;
  try {
    try { await navigator.clipboard.writeText(file.content); }
    catch { $('answer-code').focus();$('answer-code').select();if(!document.execCommand('copy'))throw new Error('剪贴板不可用，请选中代码手动复制'); }
    $('answer-status').textContent='答案代码已复制';
  } catch(e){$('answer-status').textContent=e.message;}
}
async function importAnswer(fragment=false) {
  const file=selectedAnswer(), target=$('answer-target').value, game=state.game;
  if(!file||answerGame!==game||state.busy||answerImporting)return;
  if(!state.files.some(f=>f.path===target&&!f.read_only))return;
  const area=$('answer-code'), text=fragment?area.value.slice(area.selectionStart,area.selectionEnd):file.content;
  if(!text||(!fragment&&file.path.split('.').pop().toLowerCase()!==target.split('.').pop().toLowerCase()))return;
  answerImporting=true;state.busy=true;updateButtons();
  try {
    await flushDraft();
    if(game!==state.game||answerGame!==game)return;
    const sameFile=$('files').value===target;
    selectFile(target);
    const range=fragment&&sameFile?editor.getSelection():fragment?new monaco.Range(1,1,1,1):editor.getModel().getFullModelRange();
    editor.pushUndoStop();editor.executeEdits('answer-import',[{range,text,forceMoveMarkers:true}]);editor.pushUndoStop();
    await flushDraft();$('answer-dialog').close();notice('答案已导入本地草稿，可用 Ctrl+Z 撤销；代码会后台同步，请检查后自行提交评测。');editor.focus();
  } catch(e){$('answer-status').textContent=e.message;}
  finally {answerImporting=false;state.busy=false;updateButtons();}
}
editor.onDidChangeModelContent(() => { updateButtons(); clearTimeout(draftTimer); draftTimer=setTimeout(()=>flushDraft().catch(e=>error(e.message)),500); scheduleSync(); });
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, ()=>flushDraft().then(()=>synchronize()).catch(e=>error(e.message)));

function scheduleSync() {
  clearTimeout(syncTimer);
  if(!syncFailed)syncTimer=setTimeout(()=>synchronize(),1800);
}
async function synchronize() {
  if(syncPromise)return syncPromise;
  if(state.busy||syncFailed||!state.connected||!state.task?.can_submit||!hasChanges())return;
  const game=state.game, sent=allFiles();
  syncPromise=(async()=>{
    try {
      await flushDraft();
      if(game!==state.game)return;
      await request('sync',{files:sent});
      if(game!==state.game)return;
      for(const file of sent)state.base.set(file.path,file.content);
      await flushDraft();
    }catch(e){if(game===state.game){syncFailed=true;error(`自动同步失败：${e.message}。本地草稿已保留，请刷新题目确认远端状态。`);}}
  })();updateButtons();
  try { await syncPromise; }
  finally {syncPromise=null;updateButtons();if(!syncFailed&&game===state.game&&hasChanges())scheduleSync();}
}
function pollResults() {
  clearTimeout(resultTimer);
  if(++resultAttempts>40){notice('评测尚未返回，后台查询已暂停，请手动刷新结果确认；无需重复提交。');$('remote-status').textContent='等待平台结果 · 请手动刷新';return;}
  const game=state.game;
  resultTimer=setTimeout(async()=>{
    if(game!==state.game||!state.task)return;
    if(state.busy||syncPromise){pollResults();return;}
    await operation('result');
  },3000);
}

function storeSample() {
  const sample=state.samples[state.sampleIndex]; if(sample){sample.input=$('stdin').value; sample.expected=$('expected').value || null;}
}
function showSamples() {
  options($('samples'),state.samples.map((s,index)=>({value:String(index),text:s.name})),String(state.sampleIndex));
  const sample=state.samples[state.sampleIndex]; $('stdin').value=sample?.input??''; $('expected').value=sample?.expected??'';
}
function flushDraft() {
  clearTimeout(draftTimer); storeSample();
  if(!state.game || !state.task) return draftQueue;
  const payload={ files:allFiles(), samples:structuredClone(state.samples) };
  const game=state.game;
  draftQueue=draftQueue.catch(()=>{}).then(async()=>{
    try { await request('draft',{...payload,game}); draftError=false; }
    catch(e){draftError=true; throw e;}
    finally{updateButtons();}
  });
  return draftQueue;
}

async function operation(action, payload={}) {
  if(syncPromise)await syncPromise;
  if(state.busy)return;
  if(action==='submit')celebrationArmed=true;
  error(); state.busy=true; updateButtons();
  const refreshButton=({refresh:'refresh-task',result:'refresh-result'})[action];
  if(refreshButton)refreshLoading(refreshButton,true);
  $('operation-status').textContent=({save:'正在保存',submit:'正在评测',run:'正在编译运行',result:'正在获取结果',refresh:'正在更新题目',enter:'正在进入实训'})[action]??'正在处理';
  try {
    await flushDraft();
    const sent=allFiles();
    const result=await request(action,{...payload,files:sent,entry:$('files').value,samples:structuredClone(state.samples)});
    if(action==='submit') { for(const file of sent) state.base.set(file.path,file.content); await flushDraft(); notice(result?.evaluation?.status===1?'评测仍在进行，正在后台查询结果。':'本次评测已结束');setTab('remote'); }
    if(result?.evaluation) showEvaluation(result.evaluation,action==='submit'||action==='result');
    if(result?.result_error)error(result.result_error);
    if(result?.can_submit != null && state.task) state.task.can_submit=result.can_submit;
    if(action==='run') showLocal(result);
  } catch(e) { if(action==='submit')celebrationArmed=false;if(state.task && (action==='submit'||e.message.includes('结果未知') || e.message.includes('仍在进行')))state.task.can_submit=false; error(e.message);if(action==='submit')setTab('remote'); }
  finally {if(refreshButton)refreshLoading(refreshButton,false);state.busy=false;$('operation-status').textContent='';updateButtons();if(!syncFailed&&hasChanges())scheduleSync();}
}

function resultOutput(value) {
  const block=document.createElement('div');block.className='platform-output';
  const text=String(value??'');
  if(!/<\/?(?:pre|code|span|div|p|br|table|tr|td|th|b|strong|em|ins|del)\b/i.test(text)) { const pre=document.createElement('pre');pre.textContent=text;block.append(pre);return block; }
  block.innerHTML=DOMPurify.sanitize(text,{ALLOWED_TAGS:['pre','code','span','div','p','br','table','thead','tbody','tr','td','th','b','strong','em','i','ins','del','ul','ol','li'],ALLOWED_ATTR:['style','colspan','rowspan']});
  for(const element of block.querySelectorAll('[style]')){
    const safe={};for(const key of ['color','background-color','font-weight','white-space']){
      const value=element.style.getPropertyValue(key);
      if(value&&/^[#\w\s(),.%+-]+$/.test(value)&&!/(url|var|expression)/i.test(value))safe[key]=value;
    }
    element.removeAttribute('style');for(const [key,value] of Object.entries(safe))element.style.setProperty(key,value);
  }
  return block;
}

function showEvaluation(evaluation, confirmed=false) {
  if(confirmed&&celebrationArmed&&evaluation.status!==1){celebrationArmed=false;if(evaluation.status===2)notify('celebrate-ac');}
  clearTimeout(resultTimer);if(evaluation.status!==1){if(resultAttempts)notice('本次评测已结束');resultAttempts=0;}
  const status=({0:'未开始',1:'评测中',2:'通过',3:'未通过'})[evaluation.status]??`状态 ${evaluation.status}`;
  $('remote-status').textContent=status; $('remote-status').className=evaluation.status===2?'passed':evaluation.status===3?'failed':'';
  if(evaluation.status===1)pollResults();
  $('compile-output').replaceChildren(resultOutput(evaluation.compile_output));
  $('test-cases').replaceChildren(...evaluation.tests.map(test=>{
    const item=document.createElement('details');item.className='test-case';
    const heading=document.createElement('summary'); heading.textContent=`${test.name} · ${test.public?'公开':'隐藏'} · ${test.passed===true?'通过':test.passed===false?'未通过':'尚无结果'}`;item.append(heading);
    for(const [label,text] of [['输入',test.public?test.input:null],['预期输出',test.public?test.expected:null],['实际输出',test.actual]]){
      if(text==null)continue; const title=document.createElement('div');title.className='muted';title.textContent=label;item.append(title,resultOutput(text));
    }
    if(!test.public){const p=document.createElement('p');p.className='muted';p.textContent='隐藏测试的输入和预期输出由平台保密。';item.append(p);}
    return item;
  }));
}
function showLocal(result) {
  const target=$('local-results');target.replaceChildren();
  const append=(title,text,cls='')=>{const heading=document.createElement('div');heading.textContent=title;heading.className=cls;const pre=document.createElement('pre');pre.textContent=text;target.append(heading,pre);};
  if(result.compilation){const c=result.compilation;append(c.timed_out?'编译超时':c.exit_code===0?'编译成功':'编译失败',c.stdout+c.stderr,c.exit_code===0?'passed':'failed');}
  for(const sample of result.samples??[]){const o=sample.output;append(`${sample.name} · ${o.timed_out?'运行超时':sample.passed===true?'通过':sample.passed===false?'未通过':'已运行'} · ${o.elapsed_ms} ms`, `stdout:\n${o.stdout}\n${o.stderr?'stderr:\n'+o.stderr:''}${o.truncated?'\n输出已达到大小上限。':''}`, sample.passed===true?'passed':sample.passed===false||o.timed_out?'failed':'');}
  setTab('local');
}
function updateTabIndicator(){const selected=document.querySelector('.result-tabs button.selected'),indicator=document.querySelector('.tab-indicator');if(!selected||!indicator)return;indicator.style.width=`${selected.offsetWidth}px`;indicator.style.transform=`translateX(${selected.offsetLeft}px)`;}
function setTab(tab){activeTab=tab; $('local-tab').hidden=tab!=='local';$('remote-tab').hidden=tab!=='remote';animateContent($(tab==='local'?'local-tab':'remote-tab'));document.querySelectorAll('[data-tab]').forEach(b=>b.classList.toggle('selected',b.dataset.tab===tab));updateTabIndicator();}

function receive(message) {
  if(message.reply_to){const callback=pending.get(message.reply_to);if(!callback)return;clearTimeout(callback.timer);pending.delete(message.reply_to);message.ok?callback.resolve(message.data):callback.reject(new Error(message.error??'操作失败'));return;}
  switch(message.type){
    case 'theme': document.documentElement.dataset.theme=message.dark?'dark':'light';document.documentElement.dataset.reducedMotion=String(!!message.reduce_motion); document.documentElement.style.setProperty('--accent',message.accent);monaco.editor.setTheme(message.dark?'vs-dark':'vs');break;
    case 'celebration': if(!message.game||message.game===state.game)startCelebration(message);break;
    case 'stop-celebration': stopCelebration();break;
    case 'connection': state.connected=message.connected;updateButtons();if(state.connected)scheduleSync();break;
    case 'homework':
      $('homework-title').textContent=message.title;
      options($('games'),message.games.map(g=>({value:g.identifier,text:`第 ${g.position} 关 · ${g.name}`})),message.game);
      $('enter-panel').hidden=!message.can_enter;
      if(message.can_enter){state.task=null;state.game='';state.files=[];editor.setModel(null);$('description').textContent='点击“进入实训”后可读取题目和代码。';updateButtons();}
      break;
    case 'task': {
      syncFailed=false;clearTimeout(syncTimer);
      if(state.game!==message.task.game_identifier){resetAnswers();celebrationArmed=false;stopCelebration();notify('stop-celebration');}
      const task=message.task; const previous=state.game; state.game=task.game_identifier;state.task=task;
      const drafts=new Map((message.drafts??[]).map(f=>[f.path,f.content]));
      state.files=message.files;
      for(const file of state.files){const key=`${state.game}:${file.path}`;const model=models.get(key);const preserve=previous===state.game && model && !file.read_only && !message.force;
        const content=message.force?file.content:(drafts.get(file.path)??file.content);
        if(model&&!preserve&&model.getValue()!==content)model.setValue(content);file.editor_content=content;
      }
      state.base=new Map(message.files.map(f=>[f.path,f.content]));
      $('description').innerHTML=DOMPurify.sanitize(marked.parse(normalizeTaskMarkdown(task.description_markdown??'')),{FORBID_TAGS:['iframe','form','input','button','style'],FORBID_ATTR:['style']});
      animateContent($('description'));
      options($('files'),state.files.map(f=>({value:f.path,text:f.path})),message.entry??state.files[0]?.path);
      for(const file of state.files){const key=`${state.game}:${file.path}`;if(!models.has(key))models.set(key,monaco.editor.createModel(file.editor_content,language(file.path),monaco.Uri.parse(`inmemory://educoder/${encodeURIComponent(state.game)}/${encodeURIComponent(file.path)}`)));}
      if(state.files.length)selectFile($('files').value);else{editor.setModel(null);editor.updateOptions({readOnly:true});}
      state.samples=message.samples?.length?message.samples:message.evaluation.tests.filter(t=>t.public&&t.input!=null).map((t,i)=>({name:`样例 ${i+1}`,input:t.input,expected:t.expected}));
      if(!state.samples.length)state.samples=[{name:'自定义样例 1',input:'',expected:null}];state.sampleIndex=0;showSamples();showEvaluation(message.evaluation);updateButtons();scheduleSync();notice('');break;
    }
    case 'notice': notice(message.message);break;
    case 'error': error(message.message);break;
    case 'before-close': flushDraft().then(()=>syncPromise).then(()=>notify('close-ready')).catch(e=>error(e.message));break;
    case 'before-logout': flushDraft().then(()=>syncPromise).then(()=>notify('logout-ready')).catch(e=>error(e.message));break;
    case 'navigate': if(state.busy){error('请等待当前操作完成后再切换。');break;} flushDraft().then(()=>syncPromise).then(()=>notify('navigate',message.target)).catch(e=>error(e.message));break;
    case 'clear': celebrationArmed=false;stopCelebration();notify('stop-celebration');clearTimeout(syncTimer);clearTimeout(resultTimer);syncFailed=false;resultAttempts=0;resetAnswers();clearTimeout(draftTimer);state.game='';state.task=null;state.files=[];state.base.clear();editor.setModel(null);models.forEach(m=>m.dispose());models.clear();$('description').textContent='请选择作业。';$('local-results').replaceChildren();$('test-cases').replaceChildren();updateButtons();break;
  }
}

$('files').onchange=()=>selectFile($('files').value);
$('answers').onclick=()=>loadAnswers();
$('reload-answer').onclick=()=>loadAnswers(true);
$('close-answer').onclick=()=>{if(!answerImporting)$('answer-dialog').close();};
$('answer-dialog').addEventListener('cancel',event=>{if(answerImporting)event.preventDefault();});
$('answer-files').onchange=showAnswerFile;
$('answer-target').onchange=updateAnswerButtons;
$('answer-code').addEventListener('select',updateAnswerButtons);
$('copy-answer').onclick=copyAnswer;
$('import-answer').onclick=()=>importAnswer();
$('insert-answer').onclick=()=>importAnswer(true);
$('games').onchange=async()=>{const game=$('games').value;try{await flushDraft();await syncPromise;notify('select-game',{game});}catch(e){error(e.message);$('games').value=state.game;}};
$('back').onclick=()=>flushDraft().then(()=>syncPromise).then(()=>notify('navigate',{page:'homeworks'})).catch(e=>error(e.message));
$('settings').onclick=()=>notify('settings');
for(const action of ['submit','run'])$(action).onclick=()=>operation(action);
$('refresh-result').onclick=()=>operation('result').then(()=>setTab('remote'));
$('refresh-task').onclick=()=>operation('refresh');
$('enter').onclick=()=>operation('enter');
document.querySelectorAll('[data-tab]').forEach(button=>button.onclick=()=>setTab(button.dataset.tab));
const tabObserver=new ResizeObserver(updateTabIndicator);
document.querySelectorAll('.result-tabs,.result-tabs button').forEach(element=>tabObserver.observe(element));
document.fonts.ready.then(updateTabIndicator);
$('samples').onchange=()=>{storeSample();state.sampleIndex=Number($('samples').value);showSamples();};
$('add-sample').onclick=()=>{storeSample();if(state.samples.length>=100)return;state.samples.push({name:`自定义样例 ${state.samples.length+1}`,input:'',expected:null});state.sampleIndex=state.samples.length-1;showSamples();};
$('remove-sample').onclick=()=>{if(state.samples.length<=1)return;state.samples.splice(state.sampleIndex,1);state.sampleIndex=Math.min(state.sampleIndex,state.samples.length-1);showSamples();};
for(const id of ['stdin','expected'])$(id).oninput=()=>{clearTimeout(draftTimer);draftTimer=setTimeout(()=>flushDraft().catch(e=>error(e.message)),500);};
$('description').addEventListener('click',event=>{if(event.target.closest('a')){event.preventDefault();notice('链接可通过右键复制，在浏览器中打开。');}});
const splitter=$('splitter');splitter.onpointerdown=event=>{splitter.setPointerCapture(event.pointerId);splitter.onpointermove=e=>{const percent=Math.max(25,Math.min(65,e.clientX/window.innerWidth*100));document.documentElement.style.setProperty('--problem-width',`${percent}%`);};};splitter.onpointerup=()=>{splitter.onpointermove=null;};
splitter.onkeydown=event=>{if(!['ArrowLeft','ArrowRight'].includes(event.key))return;const width=parseFloat(getComputedStyle(document.documentElement).getPropertyValue('--problem-width'))||40;document.documentElement.style.setProperty('--problem-width',`${Math.max(25,Math.min(65,width+(event.key==='ArrowRight'?2:-2)))}%`);};
window.educoder={receive,editor,flushDraft};updateButtons();notify('ready');
// WebView child-window input does not travel through Slint's popup event handler.
document.addEventListener('pointerdown',()=>notify('dismiss-account-menu'),{capture:true});
document.addEventListener('keydown',event=>{if(event.key==='Escape')notify('dismiss-account-menu');},{capture:true});
