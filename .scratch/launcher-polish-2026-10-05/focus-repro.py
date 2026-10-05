import gi, subprocess, sys, threading
from pathlib import Path
gi.require_version('Gtk', '3.0')
from gi.repository import Gtk, GLib
mode=sys.argv[1] if len(sys.argv)>1 else 'poll'
window=Gtk.Window(title='Flashcast clipboard focus reproduction')
window.set_default_size(420,160)
window.add(Gtk.Label(label='剪贴板捕获焦点检查 · 4 秒后自动关闭'))
state={'armed':False,'lost':False,'polls':0}
def lost(*_):
 if state['armed']:
  state['lost']=True
  window.hide()
  print('FAIL: clipboard polling caused focus-out and launcher hide',flush=True)
window.connect('focus-out-event',lost)
def poll():
 state['armed']=True
 def read():
  subprocess.run(([str(Path('target/debug/examples/clipboard-capture-check').resolve())] if mode == 'watcher' else ['wl-paste','--no-newline']),stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,timeout=2)
  state['polls']+=1
 if mode in ('poll','watcher'):threading.Thread(target=read,daemon=True).start()
 return True
def finish():
 print(f"mode={mode}, polls={state['polls']}, unexpected_hide={state['lost']}",flush=True)
 Gtk.main_quit()
 return False
window.show_all(); window.present()
GLib.timeout_add(900,poll)
GLib.timeout_add(4300,finish)
Gtk.main()
sys.exit(1 if state['lost'] else 0)
