#!/usr/bin/env python3
"""Serve the bounded Scufris walkthrough review UI over a JSON-lines bridge."""

from __future__ import annotations

import argparse
import html
import json
import re
import secrets
import sys
import threading
import webbrowser
from collections.abc import Callable
from dataclasses import dataclass
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any, BinaryIO, TextIO
from urllib.parse import urlparse

MAX_LINE_BYTES = 512 * 1024
MAX_INIT_LINE_BYTES = 4 * 1024 * 1024
MAX_RESULT_LINE_BYTES = 32 * 1024 * 1024
MAX_ACTION_BYTES = 16 * 1024
MAX_COMMENT_BYTES = 4096
MAX_CONTEXT_BYTES = 256 * 1024
MAX_SECTIONS = 40

CSS = r"""
:root {
  color-scheme: light dark;
  --bg: #eceeed;
  --panel: #fbfcfb;
  --ink: #1a1f20;
  --muted: #535e5d;
  --line: #c3cbc9;
  --strong: #87958f;
  --accent: #00587a;
  --ok: #17632a;
  --err: #9e1b26;
  --warn: #7a4e00;
  --add-bg: #e1f1e3;
  --del-bg: #f6e3e3;
  --hunk-bg: #e2ecf1;
  --code-bg: #f0f2f1;
  --hover: #e5e9e8;
  --on-solid: #ffffff;
}
@media (prefers-color-scheme: dark) {
  :root {
    --bg: #111514;
    --panel: #171c1b;
    --ink: #d9e1de;
    --muted: #939d99;
    --line: #2e3634;
    --strong: #4d5a57;
    --accent: #4fb3d9;
    --ok: #46a05e;
    --err: #e06058;
    --warn: #cf9a3d;
    --add-bg: #142b1c;
    --del-bg: #33191b;
    --hunk-bg: #122733;
    --code-bg: #131817;
    --hover: #1f2624;
    --on-solid: #101413;
  }
}
* { box-sizing: border-box; }
html { scroll-behavior: smooth; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--ink);
  font: 13px/1.6 ui-monospace, "SF Mono", "Cascadia Mono", Menlo, Consolas,
    "DejaVu Sans Mono", "Liberation Mono", monospace;
}
main { max-width: 1080px; margin: 0 auto; padding: 28px 20px 90px; }
:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.skip {
  position: absolute;
  left: -9999px;
  top: 0;
  z-index: 10;
  background: var(--panel);
  border: 1px solid var(--strong);
  padding: 8px 12px;
  color: var(--ink);
}
.skip:focus { left: 12px; top: 12px; }

.masthead { background: var(--panel); border: 1px solid var(--line); padding: 22px 24px; margin-bottom: 18px; }
.mast-line { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; flex-wrap: wrap; margin-bottom: 12px; }
.eyebrow { color: var(--muted); font-size: 11px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; }
.mast-rev { color: var(--muted); font-size: 12px; border: 1px solid var(--line); background: var(--code-bg); padding: 1px 8px; }
h1 { margin: 0 0 10px; font-size: 19px; line-height: 1.35; }
.facts {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  border-top: 1px solid var(--line);
  border-left: 1px solid var(--line);
  margin: 16px 0 0;
}
.fact {
  background: var(--panel);
  padding: 8px 12px;
  min-width: 0;
  border-right: 1px solid var(--line);
  border-bottom: 1px solid var(--line);
}
.fact dt { font-size: 10px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--muted); margin: 0 0 3px; }
.fact dd { margin: 0; font-size: 12px; overflow-wrap: anywhere; }
.added { color: var(--ok); font-weight: 700; }
.removed { color: var(--err); font-weight: 700; }
.passed { color: var(--ok); font-weight: 700; }
.warning { margin: 14px 0 0; padding: 8px 12px; border: 1px solid var(--warn); border-left-width: 3px; color: var(--warn); }
.mast-tools { margin-top: 16px; display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }

.index { background: var(--panel); border: 1px solid var(--line); padding: 16px 20px; margin-bottom: 18px; }
.index-head { display: flex; justify-content: space-between; align-items: baseline; gap: 12px; flex-wrap: wrap; margin-bottom: 10px; }
.index h2, .final h2 { margin: 0; font-size: 13px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; }
.index-progress { color: var(--muted); font-size: 12px; }
.progress { appearance: none; display: block; width: 100%; height: 10px; border: 1px solid var(--line); background: var(--code-bg); }
.progress::-webkit-progress-bar { background: var(--code-bg); }
.progress::-webkit-progress-value { background: var(--ok); }
.progress::-moz-progress-bar { background: var(--ok); }
.index-list { list-style: none; margin: 12px 0 0; padding: 0; border-top: 1px solid var(--line); }
.index-row {
  display: flex;
  gap: 10px;
  align-items: baseline;
  padding: 7px 4px;
  border-bottom: 1px solid var(--line);
  color: inherit;
  text-decoration: none;
  min-width: 0;
}
.index-row:hover { background: var(--hover); }
.index-mark { color: var(--muted); flex: none; }
.index-mark.done { color: var(--ok); font-weight: 700; }
.index-title { font-weight: 600; flex: 0 1 auto; min-width: 0; overflow-wrap: anywhere; }
.index-file { color: var(--muted); font-size: 12px; overflow-wrap: anywhere; min-width: 0; flex: 1; }
.kbd-hint { margin: 10px 0 0; color: var(--muted); font-size: 11px; }
kbd { border: 1px solid var(--strong); background: var(--code-bg); padding: 0 5px; font: inherit; font-size: 11px; }

.tag, .badge {
  display: inline-block;
  border: 1px solid var(--line);
  background: var(--code-bg);
  color: var(--muted);
  padding: 1px 8px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  white-space: nowrap;
}
.importance-critical { color: var(--err); border-color: var(--err); }
.importance-important { color: var(--warn); border-color: var(--warn); }
.importance-supporting { color: var(--muted); border-color: var(--strong); }
.state-not-reviewed { color: var(--muted); }
.state-looks-good { color: var(--ok); border-color: var(--ok); }
.state-needs-explanation { color: var(--accent); border-color: var(--accent); }
.state-change-requested { color: var(--err); border-color: var(--err); }

.card { background: var(--panel); border: 1px solid var(--line); margin: 0 0 18px; scroll-margin-top: 16px; }
.card.viewed { border-left: 3px solid var(--ok); }
.card.viewed .card-details { display: none; }
.card.viewed .card-head { border-bottom: 0; }
.card.viewed h2 { color: var(--muted); }
.card-head { display: flex; justify-content: space-between; align-items: flex-start; gap: 16px; padding: 16px 20px; border-bottom: 1px solid var(--line); }
.card-title { min-width: 0; }
.card-count { margin: 0 0 4px; color: var(--muted); font-size: 11px; letter-spacing: 0.1em; text-transform: uppercase; }
.card h2 { margin: 0 0 6px; font-size: 15px; }
.meta { margin: 4px 0 0; display: flex; gap: 8px; flex-wrap: wrap; align-items: baseline; min-width: 0; }
.card-file { color: var(--muted); font-size: 12px; overflow-wrap: anywhere; }
.view-control {
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  color: var(--muted);
  font-weight: 600;
  white-space: nowrap;
  flex: none;
  cursor: pointer;
}
.view-control input { width: 15px; height: 15px; margin: 0; accent-color: var(--ok); }

.prose { padding: 14px 20px; overflow-wrap: break-word; }
.prose.intro { padding: 0; }
.prose p { margin: 8px 0; }
.prose h3, .prose h4, .prose h5, .prose h6 { margin: 14px 0 6px; font-size: 13px; letter-spacing: 0.05em; text-transform: uppercase; }
.prose h3 { font-size: 14px; }
.prose ul, .prose ol { margin: 8px 0; padding-left: 22px; }
.prose li { margin: 3px 0; }
.prose blockquote { margin: 10px 0; padding: 2px 12px; border-left: 3px solid var(--strong); color: var(--muted); }
.prose hr { border: 0; border-top: 1px solid var(--line); margin: 14px 0; }
.prose code { background: var(--code-bg); border: 1px solid var(--line); padding: 0 4px; font-size: 12px; overflow-wrap: anywhere; }
.prose .md-code { margin: 10px 0; padding: 10px 12px; border: 1px solid var(--line); background: var(--code-bg); overflow-x: auto; font-size: 12px; line-height: 18px; }
.prose .md-code code { border: 0; background: none; padding: 0; }

.diff { margin: 0; border-top: 1px solid var(--line); border-bottom: 1px solid var(--line); overflow-x: auto; background: var(--code-bg); font-size: 12px; line-height: 20px; }
.diff-line { display: block; white-space: pre; min-width: max-content; padding: 0 14px; border-left: 3px solid transparent; }
.diff-add { background: var(--add-bg); border-left-color: var(--ok); }
.diff-del { background: var(--del-bg); border-left-color: var(--err); }
.diff-hunk { background: var(--hunk-bg); color: var(--accent); }
.diff-file { color: var(--muted); font-weight: 600; }

.prompt { margin: 14px 20px; padding: 10px 14px; border: 1px solid var(--line); border-left: 3px solid var(--accent); background: var(--code-bg); }
.prompt strong { display: block; color: var(--accent); font-size: 10px; letter-spacing: 0.12em; text-transform: uppercase; margin-bottom: 4px; }
.answers { margin: 0 20px; }
.answer { border: 1px solid var(--line); background: var(--code-bg); padding: 10px 14px; margin: 10px 0; }
.answer strong { display: block; margin-bottom: 4px; }
.answer div { white-space: pre-wrap; overflow-wrap: anywhere; }
.controls { padding: 14px 20px 20px; }

.comment-thread, .blocking-thread { border: 1px solid var(--line); margin: 10px 0; }
.comment-thread:empty, .blocking-thread:empty { display: none; }
.review-comment + .review-comment, .blocking-change + .blocking-change { border-top: 1px solid var(--line); }
.review-comment-head, .blocking-change-head { padding: 6px 12px; background: var(--code-bg); border-bottom: 1px solid var(--line); font-size: 11px; color: var(--muted); overflow-wrap: anywhere; }
.review-comment-body, .blocking-change-body { padding: 10px 12px; white-space: pre-wrap; overflow-wrap: anywhere; }
.blocking-thread { border-color: var(--err); }
.blocking-change-head { color: var(--err); }

.comment, .question, .overall-comment {
  width: 100%;
  border: 1px solid var(--strong);
  background: var(--panel);
  color: var(--ink);
  padding: 8px 10px;
  margin: 0 0 10px;
  font: inherit;
}
.comment, .overall-comment { min-height: 84px; resize: vertical; }
::placeholder { color: var(--muted); opacity: 0.8; }
.comment-composer, .question-composer { border-top: 1px solid var(--line); padding-top: 14px; margin-top: 14px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; margin-top: 14px; }

.button {
  appearance: none;
  border: 1px solid var(--strong);
  background: var(--panel);
  color: var(--ink);
  font: inherit;
  font-weight: 600;
  padding: 6px 14px;
  cursor: pointer;
}
.button:hover { background: var(--hover); }
.button:disabled { opacity: 0.45; cursor: not-allowed; }
.button.primary { background: var(--ok); border-color: var(--ok); color: var(--on-solid); }
.button.danger { background: var(--err); border-color: var(--err); color: var(--on-solid); }
.button.comment-action { background: var(--accent); border-color: var(--accent); color: var(--on-solid); }
.button.primary:hover, .button.danger:hover, .button.comment-action:hover { filter: brightness(1.12); }

.feedback { min-height: 20px; margin-top: 8px; color: var(--muted); font-size: 12px; }
.feedback.error { color: var(--err); font-weight: 600; }
.feedback.success { color: var(--ok); }
.feedback.compact { width: 100%; min-height: 0; margin: 0; white-space: normal; text-align: right; }
.feedback.compact:empty { display: none; }
.spinner { display: inline-block; width: 9px; height: 9px; background: currentColor; margin-right: 7px; vertical-align: -1px; animation: pulse 1s steps(2, start) infinite; }
@keyframes pulse { to { opacity: 0; } }
.context-view { max-height: 420px; overflow: auto; margin: 12px 0 0; padding: 10px 12px; border: 1px solid var(--line); background: var(--code-bg); font-size: 12px; line-height: 18px; white-space: pre; }

.final { background: var(--panel); border: 1px solid var(--line); padding: 20px 24px; margin-top: 24px; }
.final-counts { display: flex; gap: 16px; flex-wrap: wrap; color: var(--muted); font-weight: 600; font-size: 12px; margin-top: 12px; }
.review-summary { margin-top: 12px; }
.review-summary:empty { display: none; }
.overall-label { display: block; font-weight: 700; margin: 16px 0 6px; }
.final-note { color: var(--muted); font-size: 12px; margin: 10px 0; }

@media (max-width: 680px) {
  main { padding: 14px 10px 60px; }
  .masthead, .index, .final { padding: 14px; }
  .card-head { flex-direction: column; gap: 10px; }
  .view-control { justify-content: flex-start; }
  .prose, .controls { padding-left: 14px; padding-right: 14px; }
  .prompt, .answers { margin-left: 14px; margin-right: 14px; }
  .actions .button { flex: 1; }
  .index-row { flex-wrap: wrap; }
  .index-file { flex-basis: 100%; }
  .index-row .tag { margin-left: auto; }
}
@media (prefers-reduced-motion: reduce) {
  html { scroll-behavior: auto; }
  .spinner { animation: none; }
}
"""

JS = r"""
const root=document.querySelector('main');
const cards=[...root.querySelectorAll('[data-card]')];
let currentState=null;
let activeCard=-1;
const stateLabels={'not-reviewed':'Not viewed','looks-good':'Viewed','needs-explanation':'Needs explanation','change-requested':'Change requested'};
function escapeText(value){return document.createTextNode(value)}
function commentBox(item){const box=document.createElement('div');box.className='review-comment';const head=document.createElement('div');head.className='review-comment-head';head.textContent=`Comment on ${item.file}:${item.lines}`;const body=document.createElement('div');body.className='review-comment-body';body.textContent=item.body;box.append(head,body);return box}
function blockingBox(item){const box=document.createElement('div');box.className='blocking-change';const head=document.createElement('div');head.className='blocking-change-head';const anchor=document.querySelector(`[data-card="${CSS.escape(item.sectionId)}"] .meta code`)?.textContent||item.sectionId;head.textContent=`Existing change request on ${anchor}`;const body=document.createElement('div');body.className='blocking-change-body';body.textContent=item.feedback;box.append(head,body);return box}
function syncFinalActions(){
  const overall=document.querySelector('.overall-comment');const hasOverall=Boolean(overall&&overall.value.trim());const count=Number(document.querySelector('[data-note-count]')?.textContent||0);const hasComments=(currentState?currentState.comments.length:count)>0||hasOverall;
  const approve=document.querySelector('[data-action="approve"]');if(approve){approve.textContent=hasComments?'Approve with comments':'Approve';if(currentState){const viewed=Object.values(currentState.viewed).filter(Boolean).length;approve.disabled=viewed!==Object.keys(currentState.sections).length||currentState.changeRequests.length>0||currentState.approved;}}
  const request=document.querySelector('[data-action="request-changes"]');if(request)request.hidden=!hasOverall;
}
function renderState(state){
  currentState=state;
  let viewed=0;
  for(const [id,value] of Object.entries(state.sections)){
    if(state.viewed[id])viewed++;
    const card=document.querySelector(`[data-card="${CSS.escape(id)}"]`);if(card)card.classList.toggle('viewed',state.viewed[id]);
    const checkbox=document.querySelector(`[data-viewed="${CSS.escape(id)}"]`);if(checkbox)checkbox.checked=state.viewed[id];
    const mark=document.querySelector(`[data-nav-viewed="${CSS.escape(id)}"]`);if(mark){mark.textContent=state.viewed[id]?'[x]':'[ ]';mark.classList.toggle('done',state.viewed[id]);}
    const badge=document.querySelector(`[data-state="${CSS.escape(id)}"]`);
    if(badge){badge.className=`badge state-${value}`;badge.replaceChildren(escapeText(stateLabels[value]||value));}
    const answers=document.querySelector(`[data-answers="${CSS.escape(id)}"]`);
    if(answers){answers.replaceChildren();for(const item of state.questions.filter(q=>q.sectionId===id&&q.answer)){
      const box=document.createElement('div');box.className='answer';const strong=document.createElement('strong');strong.textContent=item.question;const text=document.createElement('div');text.textContent=item.answer;box.append(strong,text);answers.append(box);
    }}
    for(const thread of document.querySelectorAll(`[data-comments="${CSS.escape(id)}"]`)){thread.replaceChildren();for(const item of state.comments.filter(n=>n.sectionId===id))thread.append(commentBox(item));}
    for(const thread of document.querySelectorAll(`[data-blocks="${CSS.escape(id)}"]`)){thread.replaceChildren();for(const item of state.changeRequests.filter(n=>n.sectionId===id))thread.append(blockingBox(item));}
  }
  const total=Object.keys(state.sections).length,comments=state.comments.length,blocks=state.changeRequests.length;
  document.querySelectorAll('[data-reviewed]').forEach(x=>x.textContent=viewed);document.querySelectorAll('[data-total]').forEach(x=>x.textContent=total);document.querySelectorAll('[data-note-count]').forEach(x=>x.textContent=comments);document.querySelectorAll('[data-block-count]').forEach(x=>x.textContent=blocks);document.querySelectorAll('[data-progress]').forEach(x=>{x.max=total;x.value=viewed});
  const summary=document.querySelector('[data-review-summary]');if(summary){summary.replaceChildren();for(const item of state.comments)summary.append(commentBox(item));}
  const changeSummary=document.querySelector('[data-change-summary]');if(changeSummary){changeSummary.replaceChildren();for(const item of state.changeRequests)changeSummary.append(blockingBox(item));}
  syncFinalActions();
}
async function act(control,action=control.dataset.action){
  const scope=control.closest('[data-scope]');const feedback=scope.querySelector('.feedback');const controls=scope.querySelectorAll('button,input,textarea');const input=control.dataset.input?scope.querySelector(control.dataset.input):null;const payload={action};if(scope.dataset.section)payload.section=scope.dataset.section;if(input)payload.comment=input.value;
  controls.forEach(x=>x.disabled=true);feedback.className='feedback';feedback.innerHTML='<span class="spinner"></span>Working...';
  try{const response=await fetch('action',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(payload)});let result;try{result=await response.json()}catch{throw new Error(`Request failed (${response.status})`)}if(result.state)renderState(result.state);if(!response.ok||!result.ok)throw new Error(result.error||`Request failed (${response.status})`);if(typeof result.context==='string'){const view=scope.querySelector('.context-view');if(view){view.querySelector('code').textContent=result.context;view.hidden=false;}}feedback.textContent=result.message||'Updated.';feedback.className='feedback success';if(input&&['ask','add-comment'].includes(payload.action))input.value='';}
  catch(error){feedback.textContent=error instanceof Error?error.message:String(error);feedback.className='feedback error';}
  finally{controls.forEach(x=>x.disabled=false);if(currentState)renderState(currentState);}
}
root.addEventListener('click',event=>{const button=event.target.closest('button[data-action]');if(button){event.preventDefault();act(button)}});
root.addEventListener('change',event=>{const checkbox=event.target.closest('input[data-viewed]');if(checkbox)act(checkbox,checkbox.checked?'mark-viewed':'reopen')});
root.addEventListener('input',event=>{if(event.target.matches('.overall-comment'))syncFinalActions()});
root.addEventListener('focusin',event=>{const card=event.target.closest('[data-card]');if(card)activeCard=cards.indexOf(card)});
function focusCard(index){if(cards.length===0)return;activeCard=Math.min(cards.length-1,Math.max(0,index));const card=cards[activeCard];card.focus({preventScroll:true});card.scrollIntoView({block:'start'})}
function typingTarget(target){return target instanceof HTMLElement&&(target.isContentEditable||['INPUT','TEXTAREA','SELECT'].includes(target.tagName))}
document.addEventListener('keydown',event=>{
  if(event.defaultPrevented||event.altKey||event.ctrlKey||event.metaKey||typingTarget(event.target))return;
  if(event.key==='j'){event.preventDefault();focusCard(activeCard+1)}
  else if(event.key==='k'){event.preventDefault();focusCard(activeCard<=0?0:activeCard-1)}
  else if(event.key==='v'&&activeCard>=0){const checkbox=cards[activeCard].querySelector('input[data-viewed]');if(checkbox&&!checkbox.disabled){event.preventDefault();checkbox.checked=!checkbox.checked;act(checkbox,checkbox.checked?'mark-viewed':'reopen')}}
});
"""


def _text(value: Any, maximum: int = 262_144) -> str:
    if not isinstance(value, str) or len(value.encode("utf-8")) > maximum:
        raise ValueError("descriptor contains invalid text")
    return value


def validate_init(message: Any) -> dict[str, Any]:
    if not isinstance(message, dict) or set(message) != {
        "type",
        "version",
        "document",
        "state",
    }:
        raise ValueError("initial bridge message is invalid")
    if message["type"] != "init" or message["version"] != 1:
        raise ValueError("unsupported bridge protocol")
    document = message["document"]
    state = message["state"]
    if not isinstance(document, dict) or not isinstance(state, dict):
        raise TypeError("descriptor is invalid")
    sections = document.get("sections")
    if not isinstance(sections, list) or not 1 <= len(sections) <= MAX_SECTIONS:
        raise ValueError("descriptor sections are invalid")
    required_document = {
        "title",
        "summary",
        "revision",
        "baseRevision",
        "files",
        "added",
        "removed",
        "sections",
        "warnings",
    }
    if set(document) != required_document:
        raise ValueError("document descriptor schema is invalid")
    _text(document["title"], 400)
    _text(document["summary"])
    _text(document["revision"], 80)
    _text(document["baseRevision"], 80)
    if not all(
        isinstance(document[key], int) and document[key] >= 0
        for key in ("files", "added", "removed")
    ):
        raise ValueError("document metadata is invalid")
    ids: set[str] = set()
    for section in sections:
        required = {"id", "importance", "file", "lines", "markdown", "diff", "prompt"}
        if not isinstance(section, dict) or set(section) != required:
            raise ValueError("section descriptor schema is invalid")
        for key in required:
            _text(section[key], 262_144 if key in {"markdown", "diff"} else 4096)
        if (
            section["importance"] not in {"critical", "important", "supporting"}
            or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", section["id"])
            or section["id"] in ids
        ):
            raise ValueError("section descriptor values are invalid")
        ids.add(section["id"])
    if not isinstance(document["warnings"], list) or any(
        not isinstance(item, str) for item in document["warnings"]
    ):
        raise ValueError("warnings are invalid")
    required_state = {
        "version",
        "identity",
        "revision",
        "sections",
        "viewed",
        "questions",
        "comments",
        "changeRequests",
        "approved",
    }
    section_states = state.get("sections")
    if (
        set(state) != required_state
        or state["version"] != 1
        or not isinstance(state["identity"], str)
        or state["revision"] != document["revision"]
        or not isinstance(section_states, dict)
        or set(section_states) != ids
        or not isinstance(state["viewed"], dict)
        or set(state["viewed"]) != ids
        or any(not isinstance(item, bool) for item in state["viewed"].values())
        or any(
            value
            not in {
                "not-reviewed",
                "looks-good",
                "needs-explanation",
                "change-requested",
            }
            for value in section_states.values()
        )
        or not isinstance(state["questions"], list)
        or not isinstance(state["comments"], list)
        or not isinstance(state["changeRequests"], list)
        or not isinstance(state["approved"], bool)
    ):
        raise ValueError("state does not match document")
    for question in state["questions"]:
        if (
            not isinstance(question, dict)
            or not {"sectionId", "question"}.issubset(question)
            or not set(question).issubset({"sectionId", "question", "answer"})
            or question["sectionId"] not in ids
            or not isinstance(question["question"], str)
            or ("answer" in question and not isinstance(question["answer"], str))
        ):
            raise ValueError("state questions are invalid")
    if len(state["comments"]) > MAX_SECTIONS or len(
        {item.get("id") for item in state["comments"] if isinstance(item, dict)}
    ) != len(state["comments"]):
        raise ValueError("state comments exceed limit")
    section_by_id = {section["id"]: section for section in sections}
    for comment in state["comments"]:
        if (
            not isinstance(comment, dict)
            or set(comment) != {"id", "sectionId", "file", "lines", "body"}
            or not re.fullmatch(r"[0-9a-f]{24}", comment.get("id", ""))
            or comment.get("sectionId") not in ids
            or comment.get("file")
            != section_by_id[comment.get("sectionId", "")]["file"]
            or comment.get("lines")
            != section_by_id[comment.get("sectionId", "")]["lines"]
            or not isinstance(comment.get("body"), str)
            or not comment["body"].strip()
            or len(comment["body"].encode("utf-8")) > MAX_COMMENT_BYTES
        ):
            raise ValueError("state comments are invalid")
    for request in state["changeRequests"]:
        if (
            not isinstance(request, dict)
            or set(request) != {"sectionId", "feedback"}
            or request["sectionId"] not in ids
            or not isinstance(request["feedback"], str)
        ):
            raise ValueError("state change requests are invalid")
    if state["approved"] and (
        any(not value for value in state["viewed"].values()) or state["changeRequests"]
    ):
        raise ValueError("approved state has unresolved sections")
    return message


def validate_result(result: Any, document: dict[str, Any]) -> dict[str, Any]:
    if not isinstance(result, dict) or not isinstance(result.get("ok"), bool):
        raise TypeError("bridge result schema is invalid")
    expected = (
        {"type", "id", "ok", "state", "message"}
        if result["ok"]
        else {"type", "id", "ok", "state", "error"}
    )
    if result["ok"] and "context" in result:
        expected.add("context")
    text_key = "message" if result["ok"] else "error"
    if (
        set(result) != expected
        or result.get("type") != "result"
        or not isinstance(result.get("id"), str)
        or not isinstance(result.get(text_key), str)
        or len(result[text_key].encode("utf-8")) > MAX_LINE_BYTES
        or (
            "context" in result
            and (
                not isinstance(result["context"], str)
                or len(result["context"].encode("utf-8")) > MAX_CONTEXT_BYTES
            )
        )
    ):
        raise ValueError("bridge result schema is invalid")
    validate_init(
        {"type": "init", "version": 1, "document": document, "state": result["state"]}
    )
    return result


def read_json_line(stream: BinaryIO, maximum: int = MAX_LINE_BYTES) -> Any:
    line = stream.readline(maximum + 1)
    if not line:
        raise EOFError("bridge closed")
    if len(line) > maximum or not line.endswith(b"\n"):
        raise ValueError("bridge message exceeds limit")
    try:
        return json.loads(line)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("bridge message is malformed") from error


INLINE_CODE_PATTERN = re.compile(r"`([^`\n]+)`")
LINK_PATTERN = re.compile(r"\[([^\]\n]+)\]\(([^()\s]+)\)")
BOLD_PATTERN = re.compile(r"\*\*([^*\n]+?)\*\*")
ITALIC_PATTERN = re.compile(r"(?<!\*)\*([^*\n]+?)\*(?!\*)")
HEADING_PATTERN = re.compile(r"(#{1,6}) +(.+)")
ORDERED_PATTERN = re.compile(r"[0-9]{1,9}[.)] +(.+)")


def _emphasis(value: str) -> str:
    value = LINK_PATTERN.sub(r'\1 <code class="md-link">\2</code>', value)
    value = BOLD_PATTERN.sub(r"<strong>\1</strong>", value)
    value = ITALIC_PATTERN.sub(r"<em>\1</em>", value)
    return value


def _inline(value: str) -> str:
    escaped = html.escape(value)
    parts: list[str] = []
    position = 0
    for match in INLINE_CODE_PATTERN.finditer(escaped):
        parts.append(_emphasis(escaped[position : match.start()]))
        parts.append(f"<code>{match.group(1)}</code>")
        position = match.end()
    parts.append(_emphasis(escaped[position:]))
    return "".join(parts)


def render_markdown(value: str) -> str:
    """Render a safe structured Markdown subset to fully escaped HTML."""
    blocks: list[str] = []
    paragraph: list[str] = []
    items: list[str] = []
    list_tag = ""
    quote: list[str] = []

    def flush_paragraph() -> None:
        if paragraph:
            blocks.append(f"<p>{_inline(' '.join(paragraph))}</p>")
            paragraph.clear()

    def flush_list() -> None:
        nonlocal list_tag
        if items:
            body = "".join(f"<li>{item}</li>" for item in items)
            blocks.append(f"<{list_tag}>{body}</{list_tag}>")
            items.clear()
        list_tag = ""

    def flush_quote() -> None:
        if quote:
            blocks.append(f"<blockquote><p>{_inline(' '.join(quote))}</p></blockquote>")
            quote.clear()

    def flush() -> None:
        flush_paragraph()
        flush_list()
        flush_quote()

    lines = value.split("\n")
    index = 0
    while index < len(lines):
        stripped = lines[index].strip()
        if stripped.startswith("```"):
            flush()
            code: list[str] = []
            index += 1
            while index < len(lines) and not lines[index].strip().startswith("```"):
                code.append(lines[index])
                index += 1
            escaped = html.escape("\n".join(code))
            blocks.append(f'<pre class="md-code"><code>{escaped}</code></pre>')
        elif heading := HEADING_PATTERN.fullmatch(stripped):
            flush()
            level = min(len(heading.group(1)) + 2, 6)
            blocks.append(f"<h{level}>{_inline(heading.group(2))}</h{level}>")
        elif stripped in {"---", "***", "___"}:
            flush()
            blocks.append("<hr>")
        elif stripped.startswith(("- ", "* ", "+ ")):
            flush_paragraph()
            flush_quote()
            if list_tag != "ul":
                flush_list()
                list_tag = "ul"
            items.append(_inline(stripped[2:].strip()))
        elif ordered := ORDERED_PATTERN.fullmatch(stripped):
            flush_paragraph()
            flush_quote()
            if list_tag != "ol":
                flush_list()
                list_tag = "ol"
            items.append(_inline(ordered.group(1)))
        elif stripped.startswith(">"):
            flush_paragraph()
            flush_list()
            quote.append(stripped[1:].strip())
        elif not stripped:
            flush()
        else:
            flush_list()
            flush_quote()
            paragraph.append(stripped)
        index += 1
    flush()
    return "".join(blocks)


def diff_html(value: str) -> str:
    lines = []
    for line in value.splitlines():
        kind = "context"
        if line.startswith("@@"):
            kind = "hunk"
        elif line.startswith(("diff ", "index ", "---", "+++")):
            kind = "file"
        elif line.startswith("+"):
            kind = "add"
        elif line.startswith("-"):
            kind = "del"
        lines.append(
            f'<span class="diff-line diff-{kind}">{html.escape(line) or " "}</span>'
        )
    return "".join(lines)


STATE_LABELS = {
    "not-reviewed": "Not viewed",
    "looks-good": "Viewed",
    "needs-explanation": "Needs explanation",
    "change-requested": "Change requested",
}


def _title(section_id: str) -> str:
    return section_id.replace("-", " ").title()


def _comment_html(item: dict[str, Any]) -> str:
    return (
        '<div class="review-comment">'
        f'<div class="review-comment-head">Comment on {html.escape(item["file"])}:{html.escape(item["lines"])}</div>'
        f'<div class="review-comment-body">{html.escape(item["body"])}</div></div>'
    )


def _blocking_html(anchor: str, feedback: str) -> str:
    return (
        '<div class="blocking-change">'
        f'<div class="blocking-change-head">Existing change request on {html.escape(anchor)}</div>'
        f'<div class="blocking-change-body">{html.escape(feedback)}</div></div>'
    )


def _masthead(document: dict[str, Any]) -> str:
    warnings = "".join(
        f'<div class="warning" role="note">warn: {html.escape(item)}</div>'
        for item in document["warnings"]
    )
    return (
        '<header class="masthead">'
        '<div class="mast-line"><span class="eyebrow">scufris // quick review</span>'
        f'<code class="mast-rev" title="Reviewed revision">{html.escape(document["revision"][:12])}</code></div>'
        f"<h1>{html.escape(document['title'])}</h1>"
        f'<div class="prose intro">{render_markdown(document["summary"])}</div>'
        '<dl class="facts">'
        f'<div class="fact"><dt>revision</dt><dd><code>{html.escape(document["revision"])}</code></dd></div>'
        f'<div class="fact"><dt>base</dt><dd><code>{html.escape(document["baseRevision"])}</code></dd></div>'
        f'<div class="fact"><dt>files</dt><dd>{document["files"]}</dd></div>'
        f'<div class="fact"><dt>lines</dt><dd><span class="added">+{document["added"]}</span> <span class="removed">-{document["removed"]}</span></dd></div>'
        '<div class="fact"><dt>preflight</dt><dd class="passed">passed</dd></div>'
        "</dl>"
        f"{warnings}"
        '<div class="mast-tools" data-scope>'
        '<button class="button" data-action="full-diff">View exact full diff</button>'
        '<div class="feedback" role="status" aria-live="polite"></div>'
        "</div></header>"
    )


def _index(document: dict[str, Any], state: dict[str, Any]) -> str:
    rows = []
    for section in document["sections"]:
        section_id = html.escape(section["id"], quote=True)
        viewed = state["viewed"][section["id"]]
        rows.append(
            f'<li><a class="index-row" href="#change-{section_id}">'
            f'<span class="index-mark{" done" if viewed else ""}" data-nav-viewed="{section_id}">{"[x]" if viewed else "[ ]"}</span>'
            f'<span class="index-title">{html.escape(_title(section["id"]))}</span>'
            f'<code class="index-file">{html.escape(section["file"])}:{html.escape(section["lines"])}</code>'
            f'<span class="tag importance-{section["importance"]}">{html.escape(section["importance"])}</span>'
            "</a></li>"
        )
    reviewed = sum(state["viewed"].values())
    total = len(document["sections"])
    return (
        '<nav class="index" id="changes" aria-label="Changes">'
        f'<div class="index-head"><h2>Changes ({total})</h2>'
        f'<span class="index-progress"><span data-reviewed>{reviewed}</span>/<span data-total>{total}</span> viewed</span></div>'
        f'<progress class="progress" data-progress value="{reviewed}" max="{total}"></progress>'
        f'<ol class="index-list">{"".join(rows)}</ol>'
        '<p class="kbd-hint">keys: <kbd>j</kbd> next change / <kbd>k</kbd> previous change / <kbd>v</kbd> toggle viewed</p>'
        "</nav>"
    )


def _card(
    index: int, total: int, section: dict[str, Any], state: dict[str, Any]
) -> str:
    section_id = html.escape(section["id"], quote=True)
    value = state["sections"][section["id"]]
    answers = "".join(
        f'<div class="answer"><strong>{html.escape(item["question"])}</strong><div>{html.escape(item["answer"])}</div></div>'
        for item in state.get("questions", [])
        if item.get("sectionId") == section["id"] and item.get("answer")
    )
    comments = "".join(
        _comment_html(item)
        for item in state.get("comments", [])
        if item.get("sectionId") == section["id"]
    )
    anchor = f"{section['file']}:{section['lines']}"
    blocking = "".join(
        _blocking_html(anchor, item["feedback"])
        for item in state.get("changeRequests", [])
        if item.get("sectionId") == section["id"]
    )
    viewed = state["viewed"][section["id"]]
    return (
        f'<article class="card{" viewed" if viewed else ""}" id="change-{section_id}" data-card="{section_id}" tabindex="-1">'
        '<header class="card-head"><div class="card-title">'
        f'<p class="card-count">change {index} of {total}</p>'
        f"<h2>{html.escape(_title(section['id']))}</h2>"
        f'<p class="meta"><code class="card-file">{html.escape(section["file"])}:{html.escape(section["lines"])}</code></p>'
        f'<p class="meta tags"><span class="tag importance-{section["importance"]}">{html.escape(section["importance"])}</span>'
        f'<span class="badge state-{value}" data-state="{section_id}">{STATE_LABELS[value]}</span></p>'
        "</div>"
        f'<label class="view-control" data-scope data-section="{section_id}">'
        f'<input type="checkbox" data-viewed="{section_id}"{" checked" if viewed else ""}><span>Viewed</span>'
        '<span class="feedback compact" role="status" aria-live="polite"></span></label></header>'
        '<div class="card-details">'
        f'<div class="prose">{render_markdown(section["markdown"])}</div>'
        f'<pre class="diff" tabindex="0" aria-label="Git diff">{diff_html(section["diff"])}</pre>'
        f'<div class="prompt"><strong>Review prompt</strong>{html.escape(section["prompt"])}</div>'
        f'<div class="answers" data-answers="{section_id}">{answers}</div>'
        f'<div class="controls" data-scope data-section="{section_id}">'
        f'<div class="blocking-thread" data-blocks="{section_id}">{blocking}</div>'
        f'<div class="comment-thread" data-comments="{section_id}">{comments}</div>'
        '<div class="comment-composer"><textarea class="comment" maxlength="4096" placeholder="Leave a comment on this section" aria-label="Section review comment"></textarea>'
        '<button class="button comment-action" data-action="add-comment" data-input=".comment">Add comment</button></div>'
        '<div class="actions"><button class="button" data-action="explain">Explain review prompt</button>'
        '<button class="button" data-action="context">Show context</button></div>'
        '<div class="question-composer"><input class="question" maxlength="4096" placeholder="Ask a question about this exact revision" aria-label="Question for reviewer">'
        '<button class="button" data-action="ask" data-input=".question">Ask reviewer</button></div>'
        '<div class="feedback" role="status" aria-live="polite"></div>'
        '<pre class="context-view" aria-label="Exact-revision file context" hidden><code></code></pre>'
        "</div></div></article>"
    )


def _final(document: dict[str, Any], state: dict[str, Any]) -> str:
    reviewed = sum(state["viewed"].values())
    total = len(document["sections"])
    comments = state.get("comments", [])
    blocks = state.get("changeRequests", [])
    anchors = {
        section["id"]: f"{section['file']}:{section['lines']}"
        for section in document["sections"]
    }
    summary_notes = "".join(_comment_html(item) for item in comments)
    blocking_summary = "".join(
        _blocking_html(
            anchors.get(item["sectionId"], item["sectionId"]), item["feedback"]
        )
        for item in blocks
    )
    can_approve = reviewed == total and not blocks and not state["approved"]
    approval_label = "Approve with comments" if comments else "Approve"
    return (
        '<section class="final" data-scope>'
        "<h2>Final review</h2>"
        '<div class="final-counts">'
        f"<span><span data-reviewed>{reviewed}</span>/<span data-total>{total}</span> viewed</span>"
        f"<span><span data-note-count>{len(comments)}</span> section comments</span>"
        f"<span><span data-block-count>{len(blocks)}</span> existing change requests</span></div>"
        f'<div class="review-summary comment-thread" data-review-summary>{summary_notes}</div>'
        f'<div class="blocking-thread" data-change-summary>{blocking_summary}</div>'
        '<label class="overall-label" for="overall-review-comment">Overall review comment</label>'
        '<textarea id="overall-review-comment" class="overall-comment" maxlength="4096" placeholder="Leave an optional approval comment, or explain why changes are needed" aria-label="Overall review comment"></textarea>'
        '<p class="final-note">Approval requires every section to be viewed. Request changes requires an overall explanation and includes any existing section change requests shown above.</p>'
        f'<div class="actions"><button class="button primary" data-action="approve" data-input=".overall-comment" {"" if can_approve else "disabled"}>{approval_label}</button>'
        '<button class="button danger" data-action="request-changes" data-input=".overall-comment" hidden>Request changes</button></div>'
        '<div class="feedback" role="status" aria-live="polite"></div>'
        "</section>"
    )


def render_page(document: dict[str, Any], state: dict[str, Any]) -> str:
    total = len(document["sections"])
    cards = "".join(
        _card(index, total, section, state)
        for index, section in enumerate(document["sections"], 1)
    )
    return (
        '<!doctype html><html><head><meta charset="utf-8">'
        '<meta name="viewport" content="width=device-width,initial-scale=1">'
        '<meta name="referrer" content="no-referrer">'
        f"<title>{html.escape(document['title'])}</title>"
        '<link rel="stylesheet" href="style.css"></head><body>'
        '<a class="skip" href="#changes">Skip to changes</a>'
        f"<main>{_masthead(document)}{_index(document, state)}{cards}{_final(document, state)}</main>"
        '<script src="app.js" defer></script></body></html>'
    )


@dataclass
class Pending:
    event: threading.Event
    result: dict[str, Any] | None = None


class Bridge:
    def __init__(
        self,
        reader: BinaryIO,
        writer: TextIO,
        opener: Callable[[], Any] = lambda: None,
        shutdown: Callable[[], Any] = lambda: None,
    ) -> None:
        self.reader = reader
        self.writer = writer
        self.opener = opener
        self.shutdown = shutdown
        self.lock = threading.Lock()
        self.pending: dict[str, Pending] = {}
        self.closed = False
        self.activated = False
        self.shutdown_requested = False
        self.thread = threading.Thread(target=self._read_results, daemon=True)

    def start(self) -> None:
        self.thread.start()

    def request(self, action: dict[str, Any]) -> dict[str, Any]:
        request_id = secrets.token_hex(12)
        pending = Pending(threading.Event())
        with self.lock:
            if self.closed:
                raise RuntimeError("review bridge closed")
            self.pending[request_id] = pending
            self.writer.write(
                json.dumps(
                    {"type": "action", "id": request_id, **action},
                    separators=(",", ":"),
                )
                + "\n"
            )
            self.writer.flush()
        pending.event.wait()
        if pending.result is None:
            raise RuntimeError("review bridge closed")
        return pending.result

    def _read_results(self) -> None:
        try:
            while True:
                message = read_json_line(self.reader, MAX_RESULT_LINE_BYTES)
                if message == {"type": "shutdown"} and not self.shutdown_requested:
                    self.shutdown_requested = True
                    self.shutdown()
                    continue
                if message == {"type": "activate"} and not self.activated:
                    self.activated = True
                    try:
                        self.opener()
                    except (OSError, webbrowser.Error):
                        continue
                    continue
                if (
                    not isinstance(message, dict)
                    or message.get("type") != "result"
                    or not isinstance(message.get("id"), str)
                ):
                    raise ValueError("bridge result is invalid")
                with self.lock:
                    pending = self.pending.pop(message["id"], None)
                if pending is None:
                    raise ValueError("bridge result id is unknown")
                pending.result = message
                pending.event.set()
        except (EOFError, ValueError):
            with self.lock:
                self.closed = True
                waiting = list(self.pending.values())
                self.pending.clear()
            for pending in waiting:
                pending.event.set()


class ReviewServer(ThreadingHTTPServer):
    daemon_threads = True
    document: dict[str, Any]
    state: dict[str, Any]
    token: str
    bridge: Bridge
    state_lock: threading.Lock
    action_condition: threading.Condition
    active_actions: int
    shutting_down: bool
    shutdown_thread: threading.Thread | None

    def begin_action(self) -> bool:
        with self.action_condition:
            if self.shutting_down:
                return False
            self.active_actions += 1
            return True

    def end_action(self) -> None:
        with self.action_condition:
            self.active_actions -= 1
            self.action_condition.notify_all()

    def request_shutdown(self) -> None:
        with self.action_condition:
            self.shutting_down = True
            if self.shutdown_thread is not None:
                return
            self.shutdown_thread = threading.Thread(
                target=self._finish_shutdown,
                name="scufris-quick-review-shutdown",
            )
            thread = self.shutdown_thread
        thread.start()

    def _finish_shutdown(self) -> None:
        with self.action_condition:
            while self.active_actions:
                self.action_condition.wait()
        self.shutdown()


class Handler(BaseHTTPRequestHandler):
    server: ReviewServer

    def log_message(self, format: str, *args: object) -> None:
        return

    def _headers(self, status: HTTPStatus, content_type: str) -> None:
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header(
            "Content-Security-Policy",
            "default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'none'; font-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'",
        )
        self.end_headers()

    def _send(self, status: HTTPStatus, body: bytes, content_type: str) -> None:
        self._headers(status, content_type)
        self.wfile.write(body)

    def do_GET(self) -> None:
        path = urlparse(self.path).path
        prefix = f"/{self.server.token}/"
        if path == prefix:
            with self.server.state_lock:
                page = render_page(self.server.document, self.server.state)
            self._send(HTTPStatus.OK, page.encode(), "text/html; charset=utf-8")
        elif path == prefix + "style.css":
            self._send(HTTPStatus.OK, CSS.encode(), "text/css; charset=utf-8")
        elif path == prefix + "app.js":
            self._send(HTTPStatus.OK, JS.encode(), "text/javascript; charset=utf-8")
        else:
            self._send(HTTPStatus.NOT_FOUND, b"Not found", "text/plain; charset=utf-8")

    def do_POST(self) -> None:
        if urlparse(self.path).path != f"/{self.server.token}/action":
            self._send(
                HTTPStatus.NOT_FOUND,
                b'{"ok":false,"error":"Not found"}',
                "application/json",
            )
            return
        if not self.server.begin_action():
            self._send(
                HTTPStatus.SERVICE_UNAVAILABLE,
                b'{"ok":false,"error":"Review is closing"}',
                "application/json; charset=utf-8",
            )
            return
        try:
            self._handle_action()
        finally:
            self.server.end_action()

    def _handle_action(self) -> None:
        try:
            length = int(self.headers.get("Content-Length", "-1"))
            if length < 0 or length > MAX_ACTION_BYTES:
                raise ValueError("action body exceeds limit")
            value = json.loads(self.rfile.read(length))
            if (
                not isinstance(value, dict)
                or not set(value).issubset({"action", "section", "comment"})
                or not isinstance(value.get("action"), str)
            ):
                raise ValueError("action request is invalid")
            comment = value.get("comment", "")
            if (
                not isinstance(comment, str)
                or len(comment.encode()) > MAX_COMMENT_BYTES
            ):
                raise ValueError("review comment exceeds 4 KiB")
            for key in ("section",):
                if key in value and not isinstance(value[key], str):
                    raise ValueError("action request is invalid")
            result = validate_result(
                self.server.bridge.request(value), self.server.document
            )
            with self.server.state_lock:
                self.server.state = result["state"]
            status = HTTPStatus.OK if result.get("ok") else HTTPStatus.BAD_REQUEST
            self._send(
                status,
                json.dumps(result, separators=(",", ":")).encode(),
                "application/json; charset=utf-8",
            )
        except (TypeError, ValueError, json.JSONDecodeError) as error:
            self._send(
                HTTPStatus.BAD_REQUEST,
                json.dumps({"ok": False, "error": str(error)}).encode(),
                "application/json; charset=utf-8",
            )


def serve(reader: BinaryIO, writer: TextIO, open_browser: Callable[[str], Any]) -> int:
    descriptor = validate_init(read_json_line(reader, MAX_INIT_LINE_BYTES))
    server = ReviewServer(("127.0.0.1", 0), Handler)
    server.document = descriptor["document"]
    server.state = descriptor["state"]
    server.token = secrets.token_urlsafe(32)
    server.state_lock = threading.Lock()
    server.action_condition = threading.Condition()
    server.active_actions = 0
    server.shutting_down = False
    server.shutdown_thread = None
    host, port = server.server_address
    url = f"http://{host}:{port}/{server.token}/"
    bridge = Bridge(
        reader,
        writer,
        lambda: open_browser(url),
        server.request_shutdown,
    )
    server.bridge = bridge
    writer.write(
        json.dumps({"type": "ready", "url": url}, separators=(",", ":")) + "\n"
    )
    writer.flush()
    bridge.start()
    try:
        server.serve_forever()
    finally:
        server.server_close()
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--no-open", action="store_true", help="do not open a browser")
    args = parser.parse_args()
    opener: Callable[[str], Any] = (
        (lambda _url: None)
        if args.no_open
        else (lambda url: webbrowser.open(url, new=2))
    )
    try:
        return serve(sys.stdin.buffer, sys.stdout, opener)
    except (EOFError, TypeError, ValueError) as error:
        print(str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
