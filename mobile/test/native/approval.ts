import { beforeEach, expect, mock, test } from 'bun:test';
import { rn } from '../rn';
import type { Card } from '../../src/remote/hub';
import { askFrom } from '../../src/ui/parse';
import { setLanguage } from '../../src/i18n';
setLanguage('zh');
mock.module('react-native', () => rn());
mock.module('expo-local-authentication', () => ({ hasHardwareAsync: async () => true, authenticateAsync: async () => ({ success:true }) }));
mock.module('../../src/store/settings', () => ({ readSettings:async()=>({ lock:false, confirmDanger:false }) }));
let sent: {op:string; fields:any}[] = [];
let answer: (() => Promise<void>) | null = null;
let send: (() => Promise<void>) | null = null;
let refreshes = 0;
let clearAfter = 1;
let changed = false;
const card = {id:'t',session:'s',approval:'approve?'} as Card;
mock.module('../../src/remote/hub', () => ({
  NOT_YET:'old',
  write: async (_machine:string, op:string, fields:any) => { sent.push({op,fields}); if(op==='answer') await answer?.(); if(op==='send') await send?.(); },
  refresh: async () => { refreshes++; },
  getMachine: () => ({status:'online',cards:[{...card,session:changed?'different':'s',approval:refreshes>=clearAfter?'':card.approval}]}),
}));
const {approve,deny} = await import('../../src/actions');
const ask = askFrom(null,'approve?');
beforeEach(()=>{sent=[];answer=null;send=null;refreshes=0;clearAfter=1;changed=false;});

test('a concurrent allow blocks denial and sends no explanation', async()=>{
  let release!:()=>void;
  answer=()=>new Promise<void>(r=>release=r);
  const allowing=approve('m',card,ask);
  await new Promise(r=>setTimeout(r,0));
  expect(await deny('m',card,ask,'do not run')).toBe(false);
  expect(sent.map(x=>x.op)).toEqual(['answer']);
  expect(sent[0].fields.allow).toBe(true);
  release(); await allowing;
});
test('failed denial sends no explanation', async()=>{
  answer=async()=>{throw new Error('refused');};
  await expect(deny('m',card,ask,'stop')).rejects.toThrow('refused');
  expect(sent.map(x=>x.op)).toEqual(['answer']);
});
test('waits for the menu to close, then sends to the original conversation', async()=>{
  clearAfter=3;
  expect(await deny('m',card,ask,'please explain')).toBe(true);
  expect(refreshes).toBeGreaterThanOrEqual(3);
  expect(sent.map(x=>x.op)).toEqual(['answer','send']);
  expect(sent[1].fields).toEqual({terminal:'t',session:'s',text:'please explain',enter:true});
});
test('a conversation change after denial does not receive the explanation', async()=>{
  changed=true;
  await expect(deny('m',card,ask,'stop')).rejects.toThrow('原对话');
  expect(sent.map(x=>x.op)).toEqual(['answer']);
});
test('feedback delivery keeps the same approval lock until it completes', async()=>{
  let release!:()=>void;
  let sending!:()=>void;
  const started=new Promise<void>(r=>sending=r);
  send=()=>{sending();return new Promise<void>(r=>release=r);};
  const denying=deny('m',card,ask,'do not run');
  await started;
  expect(await approve('m',card,ask)).toBe(false);
  expect(sent.map(x=>x.op)).toEqual(['answer','send']);
  release();
  expect(await denying).toBe(true);
});
