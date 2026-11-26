import test from 'node:test';
import assert from 'node:assert/strict';
import {mkdtemp,writeFile,rm,access} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {zipSync,strToU8} from 'fflate';
import ExcelJS from 'exceljs';
import {PDFDocument,StandardFonts} from 'pdf-lib';
import {parseFile} from '../desktop/parser.mjs';

async function fixture(extension,bytes,callback,options={}) {
  const directory=await mkdtemp(join(tmpdir(),'personal-knowledge-parser-'));
  try{
    const path=join(directory,'中文资料'+extension);await writeFile(path,bytes);
    await callback(await parseFile({path,resources:resolve('resources'),ocr_enabled:false,...options}));
  }finally{await rm(directory,{recursive:true,force:true});}
}

test('code fragments retain Chinese text, symbols and original line positions',async()=>{
  const lines=Array.from({length:130},(_,index)=>index===83?'# 使用 LightGBM LambdaRank 排序算法':'line_'+index+' = '+index);
  await fixture('.py',lines.join('\n'),async(result)=>{
    const block=result.blocks.find((block)=>block.text.includes('LambdaRank'));
    assert.ok(block);assert.ok(block.location.line_start<=84);assert.ok(block.location.line_end>=84);
    assert.ok(result.blocks.some((block)=>block.location.line_end===130));
  });
});

test('DOCX paragraphs and table-cell text have stable paragraph citations',async()=>{
  const bytes=zipSync({'word/document.xml':strToU8('<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>推荐排序项目</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>LightGBM 的 NDCG 为 0.462</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>')});
  await fixture('.docx',bytes,async(result)=>{assert.match(result.blocks[0].text,/推荐排序项目/);assert.equal(result.blocks[0].location.label,'第 1–2 段');assert.match(result.blocks[0].text,/0\.462/);});
});

test('Office text decodes built-in and numeric XML references without resolving custom entities',async()=>{
  const bytes=zipSync({'word/document.xml':strToU8('<w:document><w:body><w:p><w:r><w:t>R&amp;D &#x4e2d;&#25991; &lt;0.5 &amp;lt; &unknown;</w:t></w:r></w:p></w:body></w:document>')});
  await fixture('.docx',bytes,async(result)=>{assert.equal(result.blocks[0].text,'R&D 中文 <0.5 &lt; &unknown;');});
});

test('PPTX slide order uses numeric slide positions and includes speaker notes',async()=>{
  const slide=(text)=>strToU8('<p:sld><p:cSld><a:p><a:r><a:t>'+text+'</a:t></a:r></a:p></p:cSld></p:sld>');
  const bytes=zipSync({'ppt/slides/slide10.xml':slide('第十页方案'),'ppt/slides/slide2.xml':slide('第二页实验'),'ppt/notesSlides/notesSlide2.xml':slide('会议决定采用 LightGBM')});
  await fixture('.pptx',bytes,async(result)=>{assert.equal(result.blocks[0].location.page,2);assert.ok(result.blocks.some((block)=>block.text.includes('会议决定')));assert.equal(result.blocks.at(-1).location.page,10);});
});

test('Excel retains cached formula values, sheet names and cell ranges',async()=>{
  const workbook=new ExcelJS.Workbook();const sheet=workbook.addWorksheet('排序效果');
  sheet.addRow(['算法','NDCG@10']);sheet.addRow(['LightGBM',0.462]);sheet.addRow(['差值',{formula:'B2-0.438',result:0.024}]);
  await fixture('.xlsx',await workbook.xlsx.writeBuffer(),async(result)=>{const block=result.blocks[0];assert.equal(block.location.sheet,'排序效果');assert.equal(block.location.cells,'A1:B3');assert.match(block.text,/0\.024/);assert.match(block.text,/LightGBM/);});
});

test('CSV handles quoted newlines and cites parsed record ranges',async()=>{
  await fixture('.csv','算法,说明\nLightGBM,"实验一\n实验二"\nLR,基线\n',async(result)=>{assert.match(result.blocks[0].text,/实验一\n实验二/);assert.equal(result.blocks[0].location.line_end,3);});
});

test('PDF text extraction returns page citations',async()=>{
  const document=await PDFDocument.create();const font=await document.embedFont(StandardFonts.Helvetica);const page=document.addPage([600,800]);
  page.drawText('LightGBM LambdaRank experiment. NDCG@10 = 0.462.',{x:40,y:700,size:18,font});
  await fixture('.pdf',await document.save(),async(result)=>{assert.equal(result.blocks[0].location.page,1);assert.match(result.blocks[0].text,/LightGBM/);assert.equal(result.ocr,false);});
});

test('notebooks index code and markdown while excluding execution output',async()=>{
  const bytes=JSON.stringify({cells:[{cell_type:'markdown',source:['# 推荐排序']},{cell_type:'code',source:['params = {"objective": "lambdarank"}'],outputs:[{text:'execution output with private tokens'}]}]});
  await fixture('.ipynb',bytes,async(result)=>{assert.equal(result.blocks.length,2);assert.match(result.blocks[1].text,/lambdarank/);assert.ok(!JSON.stringify(result).includes('private tokens'));});
});

test('a persistent parsing worker can recover after a malformed file',async()=>{
  const {spawn}=await import('node:child_process');const {createInterface}=await import('node:readline');
  const directory=await mkdtemp(join(tmpdir(),'personal-knowledge-worker-'));
  const worker=spawn(process.execPath,['desktop/parser.mjs','--server'],{stdio:['pipe','pipe','pipe']});
  try{
    const good=join(directory,'good.md');const bad=join(directory,'bad.docx');await writeFile(good,'推荐排序 LightGBM');await writeFile(bad,'broken');
    const lines=createInterface({input:worker.stdout})[Symbol.asyncIterator]();
    worker.stdin.write(JSON.stringify({path:bad})+'\n');assert.equal(JSON.parse((await lines.next()).value).ok,false);
    worker.stdin.write(JSON.stringify({path:good})+'\n');const result=JSON.parse((await lines.next()).value);assert.equal(result.ok,true);assert.match(result.blocks[0].text,/LightGBM/);
  }finally{worker.kill();await rm(directory,{recursive:true,force:true});}
});

test('scanned PDF can be read entirely with local OCR resources',{timeout:180000},async(context)=>{
  try{await access('resources/ocr/chi_sim.traineddata');await access('resources/ocr/eng.traineddata');}catch{context.skip('OCR assets have not been prepared yet');return;}
  const {createCanvas}=await import('@napi-rs/canvas');const canvas=createCanvas(1200,350);const drawing=canvas.getContext('2d');
  drawing.fillStyle='#ffffff';drawing.fillRect(0,0,1200,350);drawing.fillStyle='#111111';drawing.font='42px Arial';drawing.fillText('LightGBM LambdaRank 0.462',60,120);drawing.fillText('LR baseline 0.438',60,200);
  const document=await PDFDocument.create();const image=await document.embedPng(canvas.toBuffer('image/png'));const page=document.addPage([900,300]);page.drawImage(image,{x:0,y:0,width:900,height:262});
  await fixture('.pdf',await document.save(),async(result)=>{assert.equal(result.ocr,true);assert.match(result.blocks[0].text,/LightGBM/i);assert.match(result.blocks[0].text,/0\.462/);},{ocr_enabled:true});
});

test('large text and excessive line counts are refused before unbounded character arrays are built',async()=>{
  await assert.rejects(fixture('.txt','x'.repeat(8*1024*1024+1),()=>{}),/8 MB/);
  await assert.rejects(fixture('.txt','x\n'.repeat(100001),()=>{}),/10 万行/);
  await assert.rejects(fixture('.txt','x'.repeat(4*1024*1024+1),()=>{}),/安全上限/);
});

test('bounded chunk slicing preserves surrogate pairs and includes the last character',async()=>{
  const text='😀技术资料'.repeat(600)+'FINAL_MARKER';
  await fixture('.md',text,async(result)=>{
    assert.ok(result.blocks.length>1);
    assert.match(result.blocks.at(-1).text,/FINAL_MARKER/);
    for(const block of result.blocks) assert.ok(!/^[\uDC00-\uDFFF]|[\uD800-\uDBFF]$/.test(block.text));
  });
});

test('short Word paragraphs are grouped while keeping paragraph-range citations',async()=>{
  const paragraphs=Array.from({length:1000},(_,index)=>'<w:p><w:r><w:t>独立事实 '+(index+1)+'</w:t></w:r></w:p>').join('');
  await fixture('.docx',zipSync({'word/document.xml':strToU8('<w:document><w:body>'+paragraphs+'</w:body></w:document>')}),async(result)=>{
    assert.ok(result.blocks.length<100,'one paragraph must not automatically become one embedding');
    assert.match(result.blocks[0].location.label,/第 1–/);
    assert.ok(result.blocks.some(block=>block.text.includes('独立事实 1000')));
    assert.match(result.blocks.at(-1).location.label,/1000 段/);
  });
});

test('long text lines do not multiply content through fixed line-count overlap',async()=>{
  const source=Array.from({length:100},(_,index)=>'line_'+index+' '+'x'.repeat(990)).join('\n');
  await fixture('.txt',source,async(result)=>{
    const output=result.blocks.reduce((sum,block)=>sum+block.text.length,0);
    assert.ok(output<source.length*1.25,'overlap must remain bounded by characters');
    assert.ok(result.blocks.some(block=>block.location.line_end===100));
  });
});
