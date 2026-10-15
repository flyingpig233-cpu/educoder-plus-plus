import { test, expect } from '@playwright/test';

const frame={type:'task',task:{game_identifier:'demo-game',title:'求和',description_markdown:'## 求和\n\n读取两个整数，输出它们的和。\n\n```rust\nfn main() {}\n```\n\n<img src="x" onerror="window.hacked=true"><script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() { println!("hello"); }',read_only:false},{path:'helper.rs',content:'pub fn answer() -> i32 { 42 }',read_only:false}],drafts:[],samples:[{name:'样例 1',input:'1 2\n',expected:'3\n'}],evaluation:{status:0,compile_output:'',tests:[{name:'公开样例',public:true,input:'1 2',expected:'3',actual:null,passed:null},{name:'隐藏样例',public:false,input:'PRIVATE_INPUT',expected:'PRIVATE_EXPECTED',actual:null,passed:null}]} };

test.beforeEach(async({page})=>{
  await page.addInitScript(()=>{
    window.messages=[];window.drafts=[];
    window.ipc={postMessage(body){const message=JSON.parse(body);window.messages.push(message);
      if(message.action==='celebrate-ac'){window.celebrationCount=(window.celebrationCount??0)+1;window.educoder.receive({type:'celebration',id:window.celebrationCount,game:message.context,width:innerWidth,height:innerHeight,left:0,top:0,particles:Array.from({length:90},(_,i)=>({x:i/90,drift:(i%3)-1,delay:(i%5)*.02,speed:1.8,spin:720,size:7,color:['#70d6a1','#82b6ff','#f7cd6c'][i%3]}))});}
      if(message.action==='stop-celebration')window.educoder.receive({type:'stop-celebration'});
      if(!message.id)return;
      setTimeout(()=>{
        if(message.action==='draft'&&window.rejectDraft){window.educoder.receive({reply_to:message.id,ok:false,error:'草稿保存失败'});return;}
        if(message.action==='draft')window.drafts.push(message.payload);
        if(message.action==='sync'&&window.rejectSync){window.educoder.receive({reply_to:message.id,ok:false,error:'网络请求失败'});return;}
        if(message.action==='answers'){
          setTimeout(()=>window.educoder.receive(window.answerError?{reply_to:message.id,ok:false,error:'网络请求失败'}:{reply_to:message.id,ok:true,data:window.answerReply??{answer:null,cached:false,stale:false}}),window.answerDelay??0);return;
        }
        const data=message.action==='run'?{compilation:{exit_code:0,stdout:'',stderr:'',timed_out:false},samples:[{name:'样例 1',passed:true,output:{exit_code:0,stdout:'3\n',stderr:'',elapsed_ms:12,timed_out:false,truncated:false}}]}:
          message.action==='submit'?(window.submitReply??{can_submit:true,evaluation:{status:2,compile_output:'',tests:[]}}):message.action==='result'?{can_submit:true,evaluation:{status:2,compile_output:'<p>评测完成</p>',tests:[]}}:{};
        if(message.action==='sync'){setTimeout(()=>window.educoder.receive({reply_to:message.id,ok:true,data}),window.syncDelay??0);return;}
        window.educoder.receive({reply_to:message.id,ok:true,data});
      },message.action==='submit'?100:0);
    }};
  });
  await page.goto('/');
  await page.waitForFunction(()=>!!window.educoder?.editor);
  await page.evaluate(frame=>{
    window.educoder.receive({type:'homework',title:'示例课堂 · 程序设计',games:[{identifier:'demo-game',position:1,name:'求和'}],game:'demo-game',can_enter:false});
    window.educoder.receive({type:'connection',connected:true});window.educoder.receive(frame);
  },frame);
});

test('refresh icons track their own requests and recover after failures',async({page})=>{
  await page.evaluate(()=>{
    const original=window.ipc.postMessage;
    window.refreshRequests=[];
    window.ipc.postMessage=body=>{
      const message=JSON.parse(body);
      if(['refresh','result','answers'].includes(message.action)){window.refreshRequests.push(message);return;}
      original(body);
    };
  });
  for(const [id,label] of [['refresh-task','刷新题目'],['refresh-result','刷新结果'],['reload-answer','重新查找答案']]){
    if(id==='refresh-result')await page.click('[data-tab=remote]');
    if(id==='reload-answer'){
      await page.click('#answers');
      await page.evaluate(()=>window.educoder.receive({reply_to:window.refreshRequests.at(-1).id,ok:true,data:{answer:null}}));
    }
    const button=page.getByRole('button',{name:label,exact:true});
    await button.click();
    await expect(button).toHaveAttribute('aria-busy','true');
    await expect(button).toBeDisabled();
    await expect(button.locator('svg')).toHaveCSS('animation-name','refresh-spin');
    await page.emulateMedia({reducedMotion:'reduce'});
    await expect(button.locator('svg')).toHaveCSS('animation-name','none');
    await page.emulateMedia({reducedMotion:'no-preference'});
    await page.screenshot({path:`../../../target/ui-previews/${id}-loading.png`});
    await page.evaluate(()=>window.educoder.receive({reply_to:window.refreshRequests.at(-1).id,ok:false,error:'模拟刷新失败'}));
    await expect(button).toHaveAttribute('aria-busy','false');
    await expect(button).toBeEnabled();
    await expect(button.locator('svg')).toHaveCSS('animation-name','none');
  }
});

test('nonstandard Markdown headings render without rewriting code fences',async({page})=>{
  await page.evaluate(frame=>{frame.task.description_markdown='#标题\n\n##要求\n\n> ###引用标题\n\n```c\n#include <stdio.h>\n#define ANSWER 42\n##not a heading\n```\n\n    #indented code\n\n<pre>\n#raw code\n</pre>\n\n普通的 #文字';window.educoder.receive(frame);},structuredClone(frame));
  await expect(page.locator('#description h1')).toHaveText('标题');await expect(page.locator('#description h2')).toHaveText('要求');await expect(page.locator('#description h3')).toHaveText('引用标题');
  await expect(page.locator('#description pre').first()).toContainText('#include <stdio.h>\n#define ANSWER 42\n##not a heading');
  await expect(page.locator('#description pre').nth(1)).toContainText('#indented code');
  await expect(page.locator('#description pre').nth(2)).toContainText('#raw code');
  await page.evaluate(frame=>{frame.task.description_markdown='```html\n<pre>\n```\n\n##后续标题';window.educoder.receive(frame);},structuredClone(frame));
  await expect(page.locator('#description h2')).toHaveText('后续标题');
});

test('Monaco right-click opens its editing context menu',async({page,context})=>{
  await context.grantPermissions(['clipboard-read','clipboard-write']);
  await page.locator('#editor .view-lines').click({button:'right',position:{x:120,y:15}});
  await expect(page.locator('.monaco-menu')).toBeVisible();
  await expect(page.locator('.monaco-menu')).toContainText('Copy');
  await expect(page.locator('.monaco-menu')).toContainText('Paste');
  // Monaco paints the scrollable surface inside its Shadow DOM, not the wrapper.
  await expect(page.locator('.monaco-scrollable-element:has(.monaco-menu)')).toHaveCSS('background-color','rgb(41, 41, 41)');
  await page.screenshot({path:'../../../target/ui-previews/editor-context-menu.png'});
  await page.keyboard.press('Escape');await expect(page.locator('.monaco-menu')).not.toBeVisible();
});

test('AC celebration only plays for a new confirmed pass and does not block editing',async({page})=>{
  await page.evaluate(frame=>{frame.evaluation.status=2;window.educoder.receive(frame);},structuredClone(frame));
  await expect(page.locator('#ac-celebration')).toBeHidden();
  await page.click('#submit');await expect(page.locator('#ac-celebration')).toBeVisible();
  await expect(page.locator('.ac-toast strong')).toHaveText('AC');
  await page.waitForTimeout(600);await page.screenshot({path:'../../../target/ui-previews/accepted-confetti.png'});
  await page.evaluate(()=>window.educoder.editor.setValue('edits during celebration'));
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('edits during celebration');
  await page.click('#refresh-result');expect(await page.evaluate(()=>window.celebrationCount)).toBe(1);
  await page.evaluate(()=>window.educoder.receive({type:'clear'}));await expect(page.locator('#ac-celebration')).toBeHidden();
});

test('reduced-motion AC feedback stays static and failed evaluations do not celebrate',async({page})=>{
  await page.emulateMedia({reducedMotion:'reduce'});
  await page.evaluate(()=>window.submitReply={can_submit:true,evaluation:{status:3,compile_output:'failed',tests:[]}});
  await page.click('#submit');await expect(page.locator('#remote-status')).toHaveText('未通过');
  await expect(page.locator('#ac-celebration')).toBeHidden();
  await page.evaluate(()=>window.educoder.receive({type:'celebration',id:123,width:innerWidth,height:innerHeight,reduce_motion:true,particles:[]}));
  await expect(page.locator('#ac-celebration')).toBeVisible();
  expect(await page.locator('.ac-toast').evaluate(e=>getComputedStyle(e).animationName)).toBe('none');
});

test('automatic sync debounces edits and preserves changes typed during a request',async({page})=>{
  await expect(page.locator('#save')).toHaveCount(0);
  await page.evaluate(()=>{window.syncDelay=300;window.educoder.editor.setValue('first');window.educoder.editor.setValue('second');});
  await expect.poll(()=>page.evaluate(()=>window.messages.filter(m=>m.action==='sync').length)).toBe(1);
  await page.evaluate(()=>window.educoder.editor.setValue('newer while syncing'));
  await expect.poll(()=>page.evaluate(()=>window.messages.filter(m=>m.action==='sync').length)).toBe(2);
  await expect(page.locator('#sync-status')).toHaveText('已同步到头歌');
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('newer while syncing');
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='sync').map(m=>m.payload.files[0].content))).toEqual(['second','newer while syncing']);
  expect(await page.evaluate(()=>window.messages.some(m=>m.action==='submit'))).toBe(false);
});

test('offline and failed synchronization preserve drafts without blind retries',async({page})=>{
  await page.evaluate(()=>{window.educoder.receive({type:'connection',connected:false});window.educoder.editor.setValue('offline edits');});
  await page.waitForTimeout(2000);
  expect(await page.evaluate(()=>window.messages.some(m=>m.action==='sync'))).toBe(false);
  await page.evaluate(()=>{window.rejectSync=true;window.educoder.receive({type:'connection',connected:true});});
  await expect(page.locator('#sync-status')).toContainText('同步失败');
  await page.evaluate(()=>window.educoder.editor.setValue('keep newer draft'));
  await page.waitForTimeout(2100);
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='sync').length)).toBe(1);
  expect(await page.evaluate(()=>window.drafts.at(-1).files[0].content)).toBe('keep newer draft');
});

test('platform HTML results render safely and plain code remains literal',async({page})=>{
  await page.evaluate(frame=>{frame.evaluation={status:3,compile_output:'<pre>error &lt;stdio.h&gt;</pre><script>window.resultHacked=true</script><iframe src="https://example.com"></iframe>',tests:[{name:'HTML result',public:true,input:'1 2',expected:'<table><tr><td>42</td></tr></table>',actual:'<span style="color:red;position:fixed" onclick="window.resultHacked=true">41</span>',passed:false},{name:'plain code',public:true,input:'#include <stdio.h>',expected:'a < b',actual:null,passed:null}]};window.educoder.receive(frame);},structuredClone(frame));
  await page.click('[data-tab=remote]');
  await expect(page.locator('#compile-output pre')).toHaveText('error <stdio.h>');
  await expect(page.locator('#compile-output script,#compile-output iframe')).toHaveCount(0);
  await page.locator('#test-cases details').first().locator('summary').click();
  await expect(page.locator('#test-cases td')).toHaveText('42');
  const span=page.locator('#test-cases span');await expect(span).toHaveText('41');
  expect(await span.getAttribute('onclick')).toBeNull();expect(await span.evaluate(e=>e.style.position)).toBe('');
  expect(await page.evaluate(()=>window.resultHacked)).toBeUndefined();
  await page.locator('#test-cases details').nth(1).locator('summary').click();
  await expect(page.locator('#test-cases details').nth(1)).toContainText('#include <stdio.h>');
  await page.screenshot({path:'../../../target/ui-previews/platform-results-html.png'});
});

test('pending submission releases the UI and polls results without resubmitting',async({page})=>{
  await page.evaluate(()=>window.submitReply={can_submit:false,evaluation:{status:1,compile_output:'',tests:[]}});
  await page.click('#submit');await expect(page.locator('#notice')).toContainText('后台查询');
  await expect(page.locator('#back')).toBeEnabled();await expect(page.locator('#submit')).toBeDisabled();
  await expect(page.locator('#remote-status')).toHaveText('通过',{timeout:7000});
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='submit').length)).toBe(1);
  await expect(page.locator('#submit')).toBeEnabled();
});

const answer={title:'求和',status:'public_samples_passed',editable_paths:['main.rs'],files:[{path:'answer/求和.rs',content:'fn main() { println!("42"); }'},{path:'answer/helper.rs',content:'pub fn answer() -> i32 { 42 }'}]};

test('accepted submission with an unreadable result retains its saved baseline',async({page})=>{
  await page.evaluate(()=>{window.educoder.editor.setValue('submitted code');window.submitReply={can_submit:false,evaluation:{status:1,compile_output:'',tests:[]},result_error:'代码已提交，读取评测结果失败：字段类型不符。正在继续查询，请勿重复提交。'};});
  await page.click('#submit');await expect(page.locator('#error')).toContainText('代码已提交，读取评测结果失败');
  await expect(page.locator('#submit')).toBeDisabled();
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('submitted code');
  await expect(page.locator('#remote-status')).toHaveText('通过',{timeout:7000});
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='submit').length)).toBe(1);
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='sync').length)).toBe(0);
});

test('answers can be viewed, copied and imported with drafts and undo preserved',async({page,context})=>{
  await context.grantPermissions(['clipboard-read','clipboard-write']);
  await page.evaluate(answer=>{window.answerReply={answer,cached:true,stale:false};window.educoder.editor.setValue('original local draft');},answer);
  await page.click('#answers');await expect(page.locator('#answer-code')).toHaveValue(answer.files[0].content);
  await expect(page.locator('#answer-status')).toContainText('本地缓存');
  await page.click('#copy-answer');await expect(page.locator('#answer-status')).toContainText('已复制');
  expect(await page.evaluate(()=>navigator.clipboard.readText())).toBe(answer.files[0].content);
  await page.screenshot({path:'../../../target/ui-previews/answers-dark.png'});
  await page.evaluate(()=>window.educoder.receive({type:'theme',dark:false,accent:'#3569b5'}));
  await page.screenshot({path:'../../../target/ui-previews/answers-light.png'});
  await page.click('#import-answer');await expect(page.locator('#answer-dialog')).not.toBeVisible();
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe(answer.files[0].content);
  await expect.poll(()=>page.evaluate(()=>window.drafts.at(-1).files[0].content)).toBe(answer.files[0].content);
  expect(await page.evaluate(()=>window.drafts.some(d=>d.files[0].content==='original local draft'))).toBe(true);
  await page.evaluate(()=>window.educoder.editor.trigger('test','undo',null));
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('original local draft');
  expect(await page.evaluate(()=>window.messages.some(m=>['save','submit'].includes(m.action)))).toBe(false);
});

test('missing answers, errors and late responses from another task remain distinct',async({page})=>{
  await page.click('#answers');await expect(page.locator('#answer-status')).toHaveText('未找到对应答案');
  await expect(page.locator('#import-answer')).toBeDisabled();
  await page.evaluate(()=>window.answerError=true);await page.click('#reload-answer');
  await expect(page.locator('#answer-status')).toContainText('答案查找失败');
  await page.evaluate(answer=>{window.answerError=false;window.answerDelay=200;window.answerReply={answer};},answer);
  await page.click('#reload-answer');
  await page.evaluate(frame=>{frame.task.game_identifier='other-task';window.educoder.receive(frame);},structuredClone(frame));
  await expect(page.locator('#answer-dialog')).not.toBeVisible();
  await page.waitForTimeout(300);
  expect(await page.locator('#answer-code').inputValue()).toBe('');
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe(frame.files[0].content);
});

test('header targets accept selected fragments but not a complete C program',async({page})=>{
  await page.evaluate(({frame,answer})=>{frame.files=[{path:'src/student.h',content:'// header\n',read_only:false}];window.educoder.receive(frame);answer.files=[{path:'answer/program.c',content:'int value = 42;\nint main() {}'}];window.answerReply={answer};}, {frame:structuredClone(frame),answer:structuredClone(answer)});
  await page.click('#answers');
  await expect(page.locator('#answer-code')).toHaveValue('int value = 42;\nint main() {}');
  await expect(page.locator('#import-answer')).toBeDisabled();
  await page.evaluate(()=>{const area=document.getElementById('answer-code');area.setSelectionRange(0,15);area.dispatchEvent(new Event('select'));});
  await page.click('#insert-answer');await expect(page.locator('#answer-dialog')).not.toBeVisible();
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toContain('int value = 42;');
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).not.toContain('main()');
});

test('real Monaco, sanitized Markdown, multi-file drafts and local results',async({page})=>{
  await expect(page.locator('#description h2')).toHaveText('求和');
  await expect(page.locator('#description script')).toHaveCount(0);
  expect(await page.locator('#description img').getAttribute('onerror')).toBeNull();
  expect(await page.evaluate(()=>window.hacked)).toBeUndefined();
  expect(await page.evaluate(()=>window.educoder.editor.getModel().getLanguageId())).toBe('rust');
  await page.evaluate(()=>window.educoder.editor.setValue('fn main() { println!("3"); }'));
  await expect.poll(()=>page.evaluate(()=>window.drafts.at(-1)?.files[0]?.content)).toContain('println!("3")');
  await page.selectOption('#files','helper.rs');await page.selectOption('#files','main.rs');
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toContain('println!("3")');
  await page.click('#run');await expect(page.locator('#local-results')).toContainText('编译成功');await expect(page.locator('#local-results')).toContainText('样例 1 · 通过');
  await page.locator('[data-tab=remote]').click();await expect(page.locator('#test-cases')).not.toContainText('PRIVATE_INPUT');await expect(page.locator('#test-cases')).not.toContainText('PRIVATE_EXPECTED');
  await page.screenshot({path:'../../../target/ui-previews/workbench-dark.png'});
  await page.evaluate(()=>window.educoder.receive({type:'theme',dark:false,accent:'#3569b5'}));
  await expect(page.locator('html')).toHaveAttribute('data-theme','light');await page.screenshot({path:'../../../target/ui-previews/workbench-light.png'});
});

test('a failed draft prevents navigation and does not erase edits',async({page})=>{
  await page.evaluate(()=>{window.rejectDraft=true;window.educoder.editor.setValue('keep these edits');});
  await page.click('#back');await expect(page.locator('#error')).toContainText('草稿保存失败');
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='navigate').length)).toBe(0);
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('keep these edits');
});

test('submission is single-shot and retains edits made during evaluation',async({page})=>{
  await page.evaluate(()=>{$('submit');function $(id){const b=document.getElementById(id);b.click();b.click();}});
  await expect.poll(()=>page.evaluate(()=>window.messages.filter(m=>m.action==='submit').length)).toBe(1);
  await page.evaluate(()=>window.educoder.editor.setValue('newer edits'));
  await expect(page.locator('#notice')).toContainText('本次评测已结束');
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('newer edits');
  await expect(page.locator('#dirty')).toHaveText('本地草稿');
});

test('remote changes preserve local text and require explicit conflict resolution',async({page})=>{
  await page.evaluate(()=>window.educoder.editor.setValue('local draft'));
  await page.evaluate(frame=>{frame.files[0].content='new remote version';frame.conflict=true;window.educoder.receive(frame);},structuredClone(frame));
  await expect(page.locator('#conflict')).toBeVisible();await expect(page.locator('#submit')).toBeDisabled();
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('local draft');
  await page.click('#keep-draft');await expect(page.locator('#conflict')).toBeHidden();
  expect(await page.evaluate(()=>window.educoder.editor.getValue())).toBe('local draft');
});

test('logout and window close wait for durable drafts, with failures blocking logout',async({page})=>{
  await page.evaluate(()=>{window.rejectDraft=true;window.educoder.editor.setValue('unsaved logout text');window.educoder.receive({type:'before-logout'});});
  await expect(page.locator('#error')).toContainText('草稿保存失败');
  expect(await page.evaluate(()=>window.messages.filter(m=>m.action==='logout-ready').length)).toBe(0);
  await page.evaluate(()=>{window.rejectDraft=false;window.educoder.receive({type:'before-logout'});});
  await expect.poll(()=>page.evaluate(()=>window.messages.filter(m=>m.action==='logout-ready').length)).toBe(1);
  expect(await page.evaluate(()=>window.drafts.at(-1).files[0].content)).toBe('unsaved logout text');
  await page.evaluate(()=>{window.educoder.editor.setValue('unsaved close text');window.educoder.receive({type:'before-close'});});
  await expect.poll(()=>page.evaluate(()=>window.messages.filter(m=>m.action==='close-ready').length)).toBe(1);
  expect(await page.evaluate(()=>window.drafts.at(-1).files[0].content)).toBe('unsaved close text');
});
