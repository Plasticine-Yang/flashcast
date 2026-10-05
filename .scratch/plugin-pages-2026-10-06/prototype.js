function flashcastPrototype(config) {
  return {
    config, ready:false, page:'home', query:'', selected:0, history:[], editor:null,
    confirmId:null, actionMenu:false, feedback:'', feedbackBody:'', dark:false,
    os:/Mac/i.test(navigator.platform)?'mac':'linux', commandPlugin:'clipboard',
    shortcuts:{clipboard:{linux:'Ctrl+Alt+C',windows:'Ctrl+Alt+C',mac:'Control+Command+C'},memo:{linux:'',windows:'',mac:''},bookmark:{linux:'',windows:'',mac:''}},
    hotkeyDraft:'', shortcutError:'', clipboardEnabled:true, retention:30, capacity:500,
    plugins:[
      {id:'clipboard',title:'剪切板',subtitle:'查看复制过的文字与图片',kind:'plugin',symbol:'▤',aliases:['剪切板','剪贴板','clipboard']},
      {id:'memo',title:'备忘录',subtitle:'查找常用内容，管理文字片段',kind:'plugin',symbol:'≡',aliases:['备忘录','memo','memos']},
      {id:'bookmark',title:'Chrome 书签',subtitle:'在 Chrome 中打开收藏的链接',kind:'plugin',symbol:'◇',aliases:['Chrome 书签','书签','bookmark','bookmarks','chrome bookmarks']}
    ],
    apps:[
      {id:'code',title:'Visual Studio Code',subtitle:'代码编辑器',kind:'app',symbol:'›_',group:'软件'},
      {id:'files',title:'文件',subtitle:'浏览文件与目录',kind:'app',symbol:'▱',group:'软件'}
    ],
    memos:[
      {id:'m1',title:'收到，稍后回复',tags:['回复','工作'],body:'收到，我先看一下。\n确认细节后会尽快回复你。',time:'今天 09:18',group:'工作'},
      {id:'m2',title:'版本发布通知',tags:['发布','工作'],body:'新版已准备好，请查看更新说明。\n\n安装后请确认运行结果。如遇到问题，请记录触发步骤与系统环境。',time:'昨天 17:42',group:'工作'},
      {id:'m3',title:'周三产品评审会议邀请',tags:['会议','邀请'],body:'周三 14:30 在三楼会议室进行产品评审。\n请提前准备需要讨论的页面与问题。',time:'10 月 3 日',group:'会议'}
    ],
    bookmarks:[
      {id:'b1',title:'MDN Web Docs',body:'https://developer.mozilla.org/zh-CN/',subtitle:'developer.mozilla.org',group:'开发 / 文档',time:'今天'},
      {id:'b2',title:'The Rust Programming Language',body:'https://doc.rust-lang.org/book/',subtitle:'doc.rust-lang.org',group:'开发 / 文档',time:'昨天'},
      {id:'b3',title:'React 文档',body:'https://react.dev/',subtitle:'react.dev',group:'开发 / 文档',time:'昨天'},
      {id:'b4',title:'GitHub',body:'https://github.com/',subtitle:'github.com',group:'开发 / 工具',time:'10 月 3 日'},
      {id:'b5',title:'Figma',body:'https://www.figma.com/',subtitle:'figma.com',group:'设计 / 工具',time:'10 月 2 日'}
    ],
    clips:[
      {id:'c1',title:'周三评审安排',body:'周三 14:30 在三楼会议室进行产品评审。\n\n讨论内容：\n1. 插件入口与返回方式\n2. 剪切板文字和图片预览\n3. 备忘录编辑与保存反馈',kind:'text',time:'刚刚',group:'今天',source:'微信'},
      {id:'c2',title:'Flashcast 页面截图',body:'640 × 420',kind:'image',image:'assets/screenshot.png',time:'7 分钟前',group:'今天',source:'截图'},
      {id:'c3',title:'设计评审文档链接',body:'https://developer.mozilla.org/zh-CN/docs/Web/Accessibility',kind:'text',time:'23 分钟前',group:'今天',source:'Chrome'},
      {id:'c4',title:'安装结果确认',body:'已完成安装。快捷键和书签打开正常，正在检查剪切板记录。',kind:'text',time:'昨天 18:26',group:'昨天',source:'邮件'},
      {id:'c5',title:'终端命令',body:'pnpm dev\ncargo test -p flashcast-core',kind:'text',time:'昨天 16:10',group:'昨天',source:'终端'}
    ],
    init(){
      const params=new URLSearchParams(location.search);
      const scene=params.get('state');
      if(['memo','bookmark','clipboard','settings'].includes(scene)) this.page=scene;
      if(params.get('dark')==='1') this.dark=true;
      this.query=params.get('q')||'';
      this.hotkeyDraft=this.shortcuts[this.commandPlugin][this.os];
      this.ready=true;
      this.$nextTick(()=>this.$refs.search?.focus());
    },
    get pageTitle(){return {home:'搜索',memo:'备忘录',bookmark:'Chrome 书签',clipboard:'剪切板',settings:'设置'}[this.page]},
    get rows(){
      const q=this.query.trim().toLowerCase();
      let list=[];
      if(this.page==='home'){
        list=[...this.apps,...this.plugins.map(p=>({...p,group:'插件'}))];
        if(q){
          list=list.filter(i=>[i.title,i.subtitle,...(i.aliases||[])].some(t=>t.toLowerCase().includes(q)));
          list.push(...this.memos.filter(m=>m.tags.some(t=>t.toLowerCase()===q)).map(m=>({...m,kind:'memo-entry',symbol:'≡',subtitle:'#'+m.tags.join('  #'),group:'标签匹配'})));
          // 标签命中在前，首次回车仍进入插件。
          list.sort((a,b)=>Number(b.kind==='memo-entry')-Number(a.kind==='memo-entry'));
        }
      } else if(this.page==='memo'){
        list=this.memos.map(m=>({...m,kind:'memo',symbol:'≡',subtitle:'#'+m.tags.join('  #')}));
        if(q)list=list.filter(m=>[m.title,m.body,...m.tags].some(t=>t.toLowerCase().includes(q)));
      } else if(this.page==='bookmark'){
        list=this.bookmarks.map(b=>({...b,kind:'bookmark',symbol:'◇'}));
        if(q)list=list.filter(b=>[b.title,b.body,b.group].some(t=>t.toLowerCase().includes(q)));
      } else if(this.page==='clipboard'){
        list=this.clips.map(c=>({...c,symbol:c.kind==='image'?'▧':'¶',subtitle:c.time+' · '+c.source}));
        if(q)list=list.filter(c=>[c.title,c.body,c.kind==='image'?'图片 image':'文字 text'].some(t=>t.toLowerCase().includes(q)));
      }
      return list;
    },
    get current(){return this.rows[this.selected]||null},
    get actionLabel(){return this.page==='home'?'进入':this.page==='bookmark'?'在 Chrome 打开':'粘贴'},
    get canManage(){return this.page==='memo'},
    get editing(){return !!this.editor},
    get split(){return this.page!=='home'||['01','02','06','07','09'].includes(config.id)},
    searchChanged(){this.selected=0;this.feedback='';this.feedbackBody='';this.confirmId=null},
    select(index){if(this.editor)return;this.selected=index;this.confirmId=null},
    groupStart(index){return index===0||this.rows[index-1]?.group!==this.rows[index]?.group},
    enter(item=this.current){
      if(!item||this.editor)return;
      this.feedback='';this.feedbackBody='';this.actionMenu=false;
      if(this.page==='home'){
        if(item.kind==='app'){this.feedback='启动目标：'+item.title;return}
        this.history.push({page:'home',query:this.query,selected:this.selected});
        const tagHit=item.kind==='memo-entry';
        this.page=tagHit?'memo':item.id;
        this.query=tagHit?this.query:'';
        this.selected=tagHit?Math.max(0,this.rows.findIndex(m=>m.id===item.id)):0;
        this.$nextTick(()=>{this.$refs.search?.focus();this.$refs.search?.select()});
      }else{
        this.feedback=this.page==='bookmark'?'打开链接：'+item.title:'粘贴内容：'+item.title;
        this.feedbackBody=item.kind==='image'?'所选图片：'+item.title:item.body;
      }
    },
    goBack(){
      if(this.editor){this.editor=null;return}
      this.confirmId=null;this.actionMenu=false;this.feedback='';this.feedbackBody='';
      const previous=this.history.pop();
      this.page=previous?.page||'home';this.query=previous?.query||'';this.selected=previous?.selected||0;
      this.$nextTick(()=>this.$refs.search?.focus());
    },
    openSettings(plugin){
      this.history.push({page:this.page,query:this.query,selected:this.selected});
      this.commandPlugin=plugin||(['memo','bookmark','clipboard'].includes(this.page)?this.page:'clipboard');
      this.page='settings';this.query='';this.hotkeyDraft=this.shortcuts[this.commandPlugin][this.os];
      this.feedback='';this.feedbackBody='';
    },
    startEdit(isNew=false){
      if(this.page!=='memo')return;
      const m=isNew?null:this.current;
      if(!isNew&&!m)return;
      this.editor={id:m?.id||null,title:m?.title||'',tags:m?.tags?.join('、')||'',body:m?.body||'',error:''};
      this.actionMenu=false;this.feedback='';this.feedbackBody='';
      this.$nextTick(()=>this.$refs.title?.focus());
    },
    saveMemo(){
      if(!this.editor)return;
      if(!this.editor.title.trim()){this.editor.error='请填写标题';return}
      if(!this.editor.body.trim()){this.editor.error='请填写正文';return}
      const old=this.editor.id;
      const m={id:old||'m'+Date.now(),title:this.editor.title.trim(),body:this.editor.body,tags:this.editor.tags.split(/[、,，]/).map(t=>t.trim()).filter(Boolean),group:'工作',time:'刚刚'};
      if(old)this.memos=this.memos.map(i=>i.id===old?m:i);else this.memos.unshift(m);
      this.editor=null;
      if(!this.rows.some(i=>i.id===m.id))this.query='';
      this.selected=Math.max(0,this.rows.findIndex(i=>i.id===m.id));
      this.feedback='已保存：'+m.title;
      this.$nextTick(()=>this.$refs.search?.focus());
    },
    deleteMemo(){
      const m=this.current;if(!m)return;
      if(this.confirmId!==m.id){this.confirmId=m.id;return}
      this.memos=this.memos.filter(i=>i.id!==m.id);this.confirmId=null;
      this.selected=Math.max(0,Math.min(this.selected,this.rows.length-1));
      this.feedback='已删除：'+m.title;
    },
    shortcutChanged(){this.hotkeyDraft=this.shortcuts[this.commandPlugin][this.os];this.shortcutError='';this.feedback=''},
    canonical(raw){return raw.toLowerCase().replace(/control/g,'ctrl').replace(/command|super|meta/g,'cmd').split('+').map(s=>s.trim()).sort().join('+')},
    saveShortcut(){
      const s=this.hotkeyDraft.trim();const parts=s.split('+').map(p=>p.trim().toLowerCase());
      if(s&&(!/^[a-z]$/i.test(parts.at(-1))||parts.length<2||parts.slice(0,-1).some(p=>!['ctrl','control','alt','option','cmd','command','super','shift','win'].includes(p)))){this.shortcutError='使用 Ctrl、Alt、Command 等修饰键加字母，例如 Ctrl+Alt+C';return}
      if(s&&(this.canonical(s)===this.canonical('Alt+Space')||Object.keys(this.shortcuts).some(p=>p!==this.commandPlugin&&this.canonical(this.shortcuts[p][this.os])===this.canonical(s)))){this.shortcutError='这个组合已分配给其他命令，原快捷键仍保留';return}
      this.shortcuts[this.commandPlugin][this.os]=s;this.shortcutError='';this.feedback=s?'已保存快捷键：'+s:'已关闭该命令的快捷键';
    },
    resetShortcut(){
      this.hotkeyDraft=this.commandPlugin==='clipboard'?(this.os==='mac'?'Control+Command+C':'Ctrl+Alt+C'):'';
      this.saveShortcut();
    },
    directPlugin(id){
      this.history=[{page:'home',query:'',selected:0}];this.page=id;this.query='';this.selected=0;this.editor=null;this.actionMenu=false;this.feedback='';this.feedbackBody='';
      this.$nextTick(()=>this.$refs.search?.focus());
    },
    key(event){
      if(event.isComposing)return;
      const pressed=[event.ctrlKey?'Ctrl':'',event.altKey?'Alt':'',event.metaKey?'Command':'',event.shiftKey?'Shift':'',event.key].filter(Boolean).join('+');
      const command=Object.keys(this.shortcuts).find(id=>{const binding=this.shortcuts[id][this.os];return binding&&this.canonical(binding)===this.canonical(pressed)});
      if(command){event.preventDefault();this.directPlugin(command);return}
      if(event.key==='Escape'){
        event.preventDefault();
        if(this.confirmId){this.confirmId=null;return}
        if(this.actionMenu){this.actionMenu=false;return}
        if(this.feedback){this.feedback='';this.feedbackBody='';return}
        if(this.page==='home'){this.query='';this.searchChanged()}else this.goBack();
        return;
      }
      if(event.ctrlKey&&event.key==='Enter'&&this.editor){event.preventDefault();this.saveMemo();return}
      if(this.editor)return;
      if((event.ctrlKey||event.metaKey)&&event.key===','){event.preventDefault();this.openSettings();return}
      if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='n'&&this.canManage){event.preventDefault();this.startEdit(true);return}
      if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='e'&&this.canManage){event.preventDefault();this.startEdit();return}
      if((event.ctrlKey||event.metaKey)&&event.key.toLowerCase()==='k'&&this.page!=='settings'){event.preventDefault();this.actionMenu=!this.actionMenu;return}
      const target=event.target;
      if(target?.matches('input,textarea,select')&&target!==this.$refs.search)return;
      if(this.page==='settings')return;
      if(event.key==='ArrowDown'||event.key==='ArrowUp'){
        event.preventDefault();
        const delta=event.key==='ArrowDown'?1:-1;
        this.selected=Math.max(0,Math.min(this.rows.length-1,this.selected+delta));
        this.$nextTick(()=>document.querySelector('[data-row-selected="true"]')?.scrollIntoView({block:'nearest'}));
      }
      if(event.key==='Enter'&&!target?.closest('button,a')){event.preventDefault();this.enter()}
    }
  };
}
