import { test, expect } from '@playwright/test';

const frame={type:'task',task:{game_identifier:'demo-game',title:'求和',description_markdown:'## 求和\n\n读取两个整数，输出它们的和。\n\n```rust\nfn main() {}\n```\n\n<img src="x" onerror="window.hacked=true"><script>window.hacked=true</script>',can_submit:true},files:[{path:'main.rs',content:'fn main() { println!("hello"); }',read_only:false},{path:'helper.rs',content:'pub fn answer() -> i32 { 42 }',read_only:false}],drafts:[],samples:[{name:'样例 1',input:'1 2\n',expected:'3\n'}],evaluation:{status:0,compile_output:'',tests:[{name:'公开样例',public:true,input:'1 2',expected:'3',actual:null,passed:null},{name:'隐藏样例',public:false,input:'PRIVATE_INPUT',expected:'PRIVATE_EXPECTED',actual:null,passed:null}]} };

test.beforeEach(async({page})=>{
  await page.addInitScript(()=>{
    window.messages=[];window.drafts=[];
    window.ipc={postMessage(body){const message=JSON.parse(body);window.messages.push(message);if(!message.id)return;
      setTimeout(()=>{
        if(message.action==='draft'&&window.rejectDraft){window.educoder.receive({reply_to:message.id,ok:false,error:'草稿保存失败'});return;}
        if(message.action==='draft')window.drafts.push(message.payload);
        if(message.action==='answers'){
          setTimeout(()=>window.educoder.receive(window.answerError?{reply_to:message.id,ok:false,error:'网络请求失败'}:{reply_to:message.id,ok:true,data:window.answerReply??{answer:null,cached:false,stale:false}}),window.answerDelay??0);return;
        }
        const data=message.action==='run'?{compilation:{exit_code:0,stdout:'',stderr:'',timed_out:false},samples:[{name:'样例 1',passed:true,output:{exit_code:0,stdout:'3\n',stderr:'',elapsed_ms:12,timed_out:false,truncated:false}}]}:
          message.action==='submit'?{can_submit:true,evaluation:{status:2,compile_output:'',tests:[]}}:{};
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

const answer={title:'求和',status:'public_samples_passed',editable_paths:['main.rs'],files:[{path:'answer/求和.rs',content:'fn main() { println!("42"); }'},{path:'answer/helper.rs',content:'pub fn answer() -> i32 { 42 }'}]};

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
