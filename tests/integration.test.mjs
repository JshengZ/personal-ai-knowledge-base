import test from 'node:test';
import assert from 'node:assert/strict';
import {createServer} from 'node:http';
import {spawn} from 'node:child_process';
import {mkdtemp,mkdir,readFile,writeFile,rename,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {setTimeout as delay} from 'node:timers/promises';
import {Client} from '@modelcontextprotocol/sdk/client/index.js';
import {StdioClientTransport} from '@modelcontextprotocol/sdk/client/stdio.js';
import {StreamableHTTPClientTransport} from '@modelcontextprotocol/sdk/client/streamableHttp.js';

async function until(callback,label,timeout=25000) {
  const started=Date.now();let last;
  while(Date.now()-started<timeout){try{const value=await callback();if(value)return value;}catch(error){last=error;}await delay(150);}
  throw new Error('Timed out: '+label+(last?' '+last.message:''));
}

test('files, hybrid retrieval, MCP, permissions, memory and backup form a complete flow',{timeout:180000},async(context)=>{
  const directory=await mkdtemp(join(tmpdir(),'personal-knowledge-integration-'));
  const home=join(directory,'data');await mkdir(home);
  const calls=[];
  const model=createServer(async(request,response)=>{
    const chunks=[];for await(const chunk of request)chunks.push(chunk);
    const body=JSON.parse(Buffer.concat(chunks).toString('utf8')||'{}');calls.push({url:request.url,body});
    response.setHeader('Content-Type','application/json');
    if(request.url==='/v1/embeddings') {
      const vector=(text)=>/校验|重复|订单/.test(text)?[0,1,0]:/排序|LightGBM|LambdaRank|LR/.test(text)?[1,0,0]:[0,0,1];
      response.end(JSON.stringify({data:body.input.map((text,index)=>({index,embedding:vector(text)}))}));
    }else if(request.url==='/v1/chat/completions') {
      response.end(JSON.stringify({choices:[{message:{content:'排序实验采用 LightGBM，NDCG@10 为 0.462。[1]'}}]}));
    }else if(request.url==='/v1/rerank') {
      response.end(JSON.stringify({results:body.documents.map((_text,index)=>({index,relevance_score:1-index*.01}))}));
    }else{response.statusCode=404;response.end('{}');}
  });
  await new Promise((resolve)=>model.listen(0,'127.0.0.1',resolve));
  const modelUrl='http://127.0.0.1:'+model.address().port+'/v1';
  const binary=resolve('engine/target/debug/knowledge-engine'+(process.platform==='win32'?'.exe':''));
  await writeFile(join(home,'runtime.json'),JSON.stringify({parser_runtime:process.execPath,parser_script:resolve('desktop/parser.mjs'),resources:resolve('resources'),web_root:resolve('dist'),mcp_executable:binary,electron_runtime:false}));
  const service=spawn(binary,['serve','--home',home],{stdio:['ignore','ignore','pipe']});
  let stderr='';service.stderr.on('data',(bytes)=>stderr=(stderr+bytes.toString()).slice(-16000));
  let info;const clients=[];
  try {
    info=await until(async()=>{const current=JSON.parse(await readFile(join(home,'service.json'),'utf8'));if(current.pid!==service.pid)return false;return current;},'backend startup');
    const url='http://127.0.0.1:'+info.port;
    const api=async(method,path,body)=>{
      const response=await fetch(url+'/api'+path,{method,headers:{Authorization:'Bearer '+info.token,...(body!==undefined?{'Content-Type':body instanceof Uint8Array?'application/octet-stream':'application/json'}:{})},body:body===undefined?undefined:body instanceof Uint8Array?body:JSON.stringify(body)});
      const value=await response.json();if(!response.ok)throw new Error(path+': '+value.error);return value;
    };
    await until(()=>api('GET','/health'),'HTTP health');
    await api('PATCH','/settings',{onboarding_complete:true,file_quiet_seconds:0,embedding:{base_url:modelUrl,model:'test-embedding',protocol:'openai'},chat:{base_url:modelUrl,model:'test-company-model',protocol:'openai'},rerank:{base_url:modelUrl,model:'test-rerank',protocol:'openai'}});
    const publicPath=join(directory,'推荐排序');const privatePath=join(directory,'个人草稿');await mkdir(publicPath);await mkdir(privatePath);
    await writeFile(join(publicPath,'排序实验.md'),'# 排序实验\nLightGBM LambdaRank 排序。NDCG@10 为 0.462，LR 为 0.438。\n');
    await writeFile(join(privatePath,'本机资料.md'),'PRIVATE_LOCAL_ONLY_DOCUMENT 仅本机草稿。');
    await api('POST','/roots',{path:publicPath,policy:'agents'});await api('POST','/roots',{path:privatePath,policy:'local'});
    let snapshot=await until(async()=>{const state=await api('GET','/state');return state.documents.length===2&&state.documents.every((document)=>document.status==='ready')?state:false;},'initial file indexing');
    const original=snapshot.documents.find((document)=>document.title==='排序实验.md');
    const project=snapshot.projects.find((project)=>project.id===original.project_id);
    await until(async()=>{const state=await api('GET','/state');return state.stats.vectors>=1&&!state.embedding_busy;},'embedding queue');
    assert.ok(!calls.filter((call)=>call.url.endsWith('/embeddings')).some((call)=>JSON.stringify(call.body).includes('PRIVATE_LOCAL_ONLY_DOCUMENT')));
    const answer=await api('POST','/ask',{query:'排序实验用了什么算法',project_id:project.id,limit:8});
    assert.match(answer.answer,/LightGBM/);assert.ok(answer.hits.length);assert.ok(answer.mode.includes('hybrid'));
    const read=await api('GET','/sources/'+original.id);assert.match(read.chunks[0].text,/0\.462/);assert.ok(read.chunks[0].location.line_start);
    const connection=await api('POST','/connections',{name:'Integration Agent',kind:'other',project_ids:[project.id],include_profile:true,can_propose:true});
    const id=connection.connection.id;
    const transport=new StdioClientTransport({command:binary,args:['mcp','--home',home,'--connection',id],env:{...process.env,...connection.server.env},stderr:'pipe'});
    const client=new Client({name:'knowledge-integration',version:'1.0.0'});clients.push(client);await client.connect(transport);
    const tools=await client.listTools();assert.deepEqual(tools.tools.map((tool)=>tool.name).sort(),['get_context','list_projects','propose_memory','read_source','search']);
    const result=await client.callTool({name:'get_context',arguments:{query:'排序算法',project_id:project.id}});
    assert.notEqual(result.isError,true);const recalled=JSON.parse(result.content[0].text);assert.ok(recalled.sources.some((source)=>source.document_id===original.id));
    const suggested=await client.callTool({name:'propose_memory',arguments:{content:'有 LightGBM 排序实验经验',category:'experience',project_id:project.id,source_ids:[original.id]}});
    const memory=JSON.parse(suggested.content[0].text);assert.equal(memory.status,'pending');
    await api('PATCH','/memories/'+memory.id,{status:'confirmed'});
    const confirmed=await client.callTool({name:'get_context',arguments:{query:'我的排序经验'}});assert.equal(JSON.parse(confirmed.content[0].text).confirmed_memories.length,1);
    await api('PATCH','/connections/'+id,{include_profile:false});
    const hidden=await client.callTool({name:'get_context',arguments:{query:'我的排序经验'}});assert.equal(JSON.parse(hidden.content[0].text).confirmed_memories.length,0);
    const httpClient=new Client({name:'knowledge-http-integration',version:'1.0.0'});clients.push(httpClient);
    await httpClient.connect(new StreamableHTTPClientTransport(new URL(connection.http.url),{requestInit:{headers:connection.http.headers}}));
    assert.equal((await httpClient.listTools()).tools.length,5);
    const httpProject=await httpClient.callTool({name:'list_projects',arguments:{}});assert.equal(JSON.parse(httpProject.content[0].text).projects.length,1);
    const version=original.version;
    await writeFile(join(publicPath,'排序实验.md'),'# 更新实验\n已改用 CatBoost 排序实验，旧方案不再使用。\n');
    await until(async()=>{const state=await api('GET','/state');return state.documents.find((document)=>document.id===original.id)?.version!==version&&state.documents.find((document)=>document.id===original.id)?.status==='ready';},'live modification');
    snapshot=await api('GET','/state');assert.equal(snapshot.memories.find((item)=>item.id===memory.id).status,'stale');
    await writeFile(join(publicPath,'新增.md'),'# 新资料\n新增检索词 QIANMING_NEW_FILE_2026。\n');
    await until(async()=>{const found=await api('POST','/search',{query:'QIANMING_NEW_FILE_2026',project_id:project.id});return found.hits.some((hit)=>hit.title==='新增.md');},'new file live indexing');
    await rename(join(publicPath,'新增.md'),join(publicPath,'重命名.md'));
    await until(async()=>{const state=await api('GET','/state');return state.documents.some((document)=>document.title==='重命名.md')&&!state.documents.some((document)=>document.title==='新增.md');},'rename reconciliation');
    await rm(join(publicPath,'重命名.md'));
    await until(async()=>{const state=await api('GET','/state');return !state.documents.some((document)=>document.title==='重命名.md');},'deletion reconciliation');
    await api('PATCH','/connections/'+id,{enabled:false});
    const revoked=await client.callTool({name:'search',arguments:{query:'排序'}});assert.equal(revoked.isError,true);
    const response=await fetch(url+'/api/backups/export',{headers:{Authorization:'Bearer '+info.token}});assert.equal(response.status,200);
    const backup=new Uint8Array(await response.arrayBuffer());assert.ok(backup.length>100);
    await api('POST','/backups/restore',backup);
    snapshot=await api('GET','/state');assert.equal(snapshot.connections.length,0);assert.ok(snapshot.roots.every((root)=>!root.enabled));assert.equal(snapshot.settings.paused,true);
    const removedDemo=await fetch(url+'/api/demo',{method:'POST',headers:{Authorization:'Bearer '+info.token,'Content-Type':'application/json'},body:'{}'});
    assert.ok([404,405].includes(removedDemo.status),'sample generation is no longer exposed');
    context.diagnostic('Verified real filesystem changes, company-compatible API flow, stdio + HTTP MCP, scope changes, citations, backup restoration and removed sample generation.');
  }catch(error){error.message+='\nBackend diagnostics: '+stderr;throw error;}
  finally {
    for(const client of clients){try{await client.close();}catch{}}
    if(info){try{await fetch('http://127.0.0.1:'+info.port+'/api/shutdown',{method:'POST',headers:{Authorization:'Bearer '+info.token}});}catch{}}
    service.kill();await new Promise((resolve)=>model.close(resolve));await rm(directory,{recursive:true,force:true});
  }
});
