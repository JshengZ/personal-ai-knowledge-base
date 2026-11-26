import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer as tcpServer} from 'node:net';
import {createServer} from 'node:http';
import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdtemp,mkdir,readFile,writeFile,rm,access} from 'node:fs/promises';
import {join,resolve,toNamespacedPath} from 'node:path';
import {tmpdir} from 'node:os';
import {setTimeout as delay} from 'node:timers/promises';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';
import {StreamableHTTPClientTransport} from '@modelcontextprotocol/sdk/client/streamableHttp.js';

const binary=resolve('engine/target/debug/knowledge-engine'+(process.platform==='win32'?'.exe':''));
const run=promisify(execFile);
async function until(fn,label,timeout=20000){const start=Date.now();let last;while(Date.now()-start<timeout){try{const result=await fn();if(result)return result;}catch(error){last=error;}await delay(100);}throw new Error(label+(last?': '+last.message:''));}
async function bounded(promise,label,ms=3000){let timer;try{return await Promise.race([promise,new Promise((_,reject)=>{timer=setTimeout(()=>reject(new Error(label+' exceeded '+ms+' ms')),ms);})]);}finally{clearTimeout(timer);}}
async function fixture(){const directory=await mkdtemp(join(tmpdir(),'pkb-resilience-'));const home=join(directory,'data');await mkdir(home);return {directory,home};}
async function runtime(home,parser=resolve('desktop/parser.mjs')){await writeFile(join(home,'runtime.json'),JSON.stringify({parser_runtime:process.execPath,parser_script:parser,resources:resolve('resources'),mcp_executable:binary,electron_runtime:false}));}
async function start(home,readHome=home){
  const child=spawn(binary,['serve','--home',toNamespacedPath(home)],{env:{...process.env,TOKIO_WORKER_THREADS:'1'},stdio:['ignore','ignore','pipe']});
  let errors='';child.stderr.on('data',bytes=>errors=(errors+bytes.toString()).slice(-3000));
  const info=await until(async()=>{const value=JSON.parse(await readFile(join(readHome,'service.json'),'utf8'));return value.pid===child.pid?value:false;},'service ready: '+errors);
  const api=async(method,path,body)=>{const response=await fetch('http://127.0.0.1:'+info.port+'/api'+path,{method,headers:{Authorization:'Bearer '+info.token,...(body===undefined?{}:{'Content-Type':'application/json'})},body:body===undefined?undefined:JSON.stringify(body),signal:AbortSignal.timeout(15000)});const value=await response.json();if(!response.ok)throw new Error(value.error);return value;};
  await api('GET','/health');return {child,info,api};
}
async function stopped(child){await until(()=>child.exitCode!==null||child.signalCode!==null,'clean service exit',7000);}
function alive(pid){try{process.kill(pid,0);return true;}catch{return false;}}

test('stdio initialize and tools/list do not contact an unresponsive backend, including Windows namespaced paths',{timeout:15000},async(context)=>{
  const {directory,home}=await fixture();const sockets=[];
  const hung=tcpServer(socket=>sockets.push(socket));await new Promise(done=>hung.listen(0,'127.0.0.1',done));
  await writeFile(join(home,'service.json'),JSON.stringify({port:hung.address().port,pid:process.pid,token:'synthetic-owner-token',version:'1.0.1'}));
  const client=new Client({name:'handshake-regression',version:'1.0.1'});
  const transport=new StdioClientTransport({command:binary,args:['mcp','--home',toNamespacedPath(home),'--connection','synthetic-connection'],env:{...process.env,PKB_CONNECTION_TOKEN:'synthetic-connection-token'},stderr:'pipe'});
  const begin=Date.now();
  try{
    await bounded(client.connect(transport),'stdio initialize');
    assert.equal((await bounded(client.listTools(),'stdio tools/list')).tools.length,5);
    assert.equal(sockets.length,0,'handshake must not wait on health checks or create a backend');
    context.diagnostic('stdio initialization + tool discovery completed in '+(Date.now()-begin)+' ms');
  }finally{await client.close().catch(()=>{});for(const socket of sockets)socket.destroy();await new Promise(done=>hung.close(done));await rm(directory,{recursive:true,force:true});}
});

test('slow embedding work leaves stdio, HTTP initialize, health and FTS responsive; migration keeps existing MCP configs valid',{timeout:90000},async(context)=>{
  const {directory,home}=await fixture();const waiting=[];let hold=true;
  const model=createServer(async(request,response)=>{const chunks=[];for await(const chunk of request)chunks.push(chunk);const body=JSON.parse(Buffer.concat(chunks).toString());if(hold)await new Promise(done=>waiting.push(done));response.setHeader('Content-Type','application/json');response.end(JSON.stringify({data:body.input.map((_,index)=>({index,embedding:[1,0,0]}))}));});
  await new Promise(done=>model.listen(0,'127.0.0.1',done));
  await runtime(home);let service=await start(home);const clients=[];
  try{
    await service.api('PATCH','/settings',{onboarding_complete:true,embedding:{base_url:'http://127.0.0.1:'+model.address().port+'/v1',model:'slow-test',protocol:'openai'}});
    const sources=join(directory,'Desktop-documents');await mkdir(sources);await writeFile(join(sources,'evidence.md'),'# RESILIENCE_NEEDLE\nA source remains readable during semantic indexing.');
    await service.api('POST','/roots',{path:sources,policy:'agents'});
    const state=await until(async()=>{const value=await service.api('GET','/state');return value.documents.length===1&&value.documents[0].status==='ready'&&value.embedding_busy?value:false;},'embedding is deliberately held');
    const config=await service.api('POST','/connections',{name:'Regression Agent',kind:'other',project_ids:state.projects.map(project=>project.id)});
    const client=new Client({name:'busy-stdio',version:'1.0.1'});clients.push(client);
    const begin=Date.now();await bounded(client.connect(new StdioClientTransport({command:binary,args:['mcp','--home',toNamespacedPath(home),'--connection',config.connection.id],env:{...process.env,...config.server.env},stderr:'pipe'})),'busy stdio initialize');
    assert.equal((await bounded(client.listTools(),'busy stdio tools/list')).tools.length,5);
    const http=new Client({name:'busy-http',version:'1.0.1'});clients.push(http);
    await bounded(http.connect(new StreamableHTTPClientTransport(new URL(config.http.url),{requestInit:{headers:config.http.headers}})),'busy HTTP initialize');
    assert.equal((await bounded(http.listTools(),'busy HTTP tools/list')).tools.length,5);
    assert.equal((await bounded(service.api('GET','/health'),'busy health')).ok,true);
    const answer=await bounded(client.callTool({name:'search',arguments:{query:'RESILIENCE_NEEDLE'}}),'FTS fallback',6000);
    assert.notEqual(answer.isError,true);const value=JSON.parse(answer.content[0].text);assert.equal(value.mode,'keyword');assert.ok(value.hits.length);assert.match(value.notice,/繁忙|关键词|全文/);
    assert.ok((await readFile(join(home,'service.log'),'utf8')).length>0,'service log is written from startup');
    context.diagnostic('All busy-state protocol operations and FTS fallback completed in '+(Date.now()-begin)+' ms with one Tokio worker');
    for(const item of clients)await item.close();clients.length=0;
    hold=false;for(const done of waiting)done();await until(async()=>{const value=await service.api('GET','/state');return value.stats.vectors===1&&!value.embedding_busy;},'embedding completes');
    await service.api('POST','/shutdown',{});await stopped(service.child);
    await mkdir(join(home,'browser'));await writeFile(join(home,'browser/cache'),'old cache');
    const target=join(directory,'other-volume-data');
    await run(binary,['migrate-data','--home',toNamespacedPath(home),'--destination',target],{timeout:20000});
    await access(join(home,'knowledge.sqlite'));
    service=await start(home,target);
    assert.equal((await service.api('GET','/state')).settings.paused,true);
    await assert.rejects(access(join(home,'knowledge.sqlite')));
    await assert.rejects(access(join(home,'browser')));
    await access(join(sources,'evidence.md'));
    const moved=new Client({name:'old-config-after-move',version:'1.0.1'});clients.push(moved);
    await bounded(moved.connect(new StdioClientTransport({command:binary,args:['mcp','--home',toNamespacedPath(home),'--connection',config.connection.id],env:{...process.env,...config.server.env},stderr:'pipe'})),'old configuration after move');
    const recalled=await bounded(moved.callTool({name:'search',arguments:{query:'RESILIENCE_NEEDLE'}}),'search after move');assert.notEqual(recalled.isError,true);assert.ok(JSON.parse(recalled.content[0].text).hits.length);
  }finally{hold=false;for(const done of waiting)done();for(const client of clients)await client.close().catch(()=>{});await service.api('POST','/shutdown',{}).catch(()=>{});await stopped(service.child).catch(()=>service.child.kill());model.closeAllConnections();await new Promise(done=>model.close(done));await rm(directory,{recursive:true,force:true});}
});

for(const forced of [false,true])test(forced?'Windows Job Object terminates parser after forced engine termination':'shutdown cancels a parser request and releases child process handles promptly',{timeout:25000,skip:forced&&process.platform!=='win32'},async()=>{
  const {directory,home}=await fixture();const pidFile=join(directory,'parser.pid');const parser=join(directory,'slow-parser.mjs');
  await writeFile(parser,"import{writeFileSync}from'node:fs';writeFileSync("+JSON.stringify(pidFile)+",String(process.pid));setInterval(()=>{},1000);process.stdin.resume();");
  await runtime(home,parser);const service=await start(home);let parserPid;
  try{
    await service.api('PATCH','/settings',{embedding_mode:'disabled'});
    const sources=join(directory,'sources');await mkdir(sources);await writeFile(join(sources,'slow.md'),'Slow parser fixture');await service.api('POST','/roots',{path:sources});
    parserPid=await until(async()=>Number(await readFile(pidFile,'utf8')),'parser starts');
    if(forced)service.child.kill('SIGKILL');else await service.api('POST','/shutdown',{});
    await stopped(service.child);await until(()=>!alive(parserPid),'parser exits and releases handles',5000);
  }finally{service.child.kill();if(parserPid&&alive(parserPid))process.kill(parserPid,'SIGKILL');await rm(directory,{recursive:true,force:true});}
});

test(
  "a company-model summary longer than twelve seconds returns its committed result, not a false timeout",
  { timeout: 40000 },
  async () => {
    const { directory, home } = await fixture();
    const model = createServer(async (request, response) => {
      const chunks = [];
      for await (const chunk of request) chunks.push(chunk);
      const body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      const evidence = JSON.parse(body.messages.at(-1).content);
      await delay(13500);
      response.setHeader("Content-Type", "application/json");
      response.end(
        JSON.stringify({
          choices: [
            {
              message: {
                content: JSON.stringify({
                  goal: "SLOW_SUMMARY_COMMITTED",
                  approach: "Use cited evidence",
                  decisions: [],
                  source_ids: evidence.sources.map((source) => source.document_id),
                  memories: [],
                }),
              },
            },
          ],
        }),
      );
    });
    await new Promise((done) => model.listen(0, "127.0.0.1", done));
    await runtime(home);
    const service = await start(home);
    try {
      await service.api("PATCH", "/settings", {
        embedding_mode: "disabled",
        auto_summarize: false,
        chat: {
          base_url: "http://127.0.0.1:" + model.address().port + "/v1",
          model: "slow-summary",
        },
      });
      const sources = join(directory, "summary-source");
      await mkdir(sources);
      await writeFile(
        join(sources, "evidence.md"),
        "# Slow summary evidence\nA long company-model request must have a definite outcome.",
      );
      await service.api("POST", "/roots", { path: sources, policy: "agents" });
      const project = await until(async () => {
        const state = await service.api("GET", "/state");
        return state.documents[0]?.status === "ready" && state.projects[0];
      }, "summary source is ready");
      const began = Date.now();
      await service.api("POST", "/projects/" + project.id + "/summarize", {});
      assert.ok(
        Date.now() - began >= 13000,
        "the test really passes the old deadline",
      );
      assert.equal(
        (await service.api("GET", "/state")).projects.find(
          (item) => item.id === project.id,
        ).goal,
        "SLOW_SUMMARY_COMMITTED",
      );
    } finally {
      await service.api("POST", "/shutdown", {}).catch(() => {});
      await stopped(service.child).catch(() => service.child.kill());
      model.closeAllConnections();
      await new Promise((done) => model.close(done));
      await rm(directory, { recursive: true, force: true });
    }
  },
);
