import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter, Routes, Route, NavLink } from 'react-router-dom';
import { Activity, LayoutDashboard, Settings, FolderGit2, Cpu, Trash2, Play, CheckCircle2, XCircle } from 'lucide-react';
import { api, Project, Executor, DashboardData, ConnectionData, ProxyData } from './api';

// --- 通用 UI 组件 ---
const Input = (props: React.InputHTMLAttributes<HTMLInputElement>) => (
  <input {...props} style={{ width: '100%', padding: '8px 12px', background: '#0b1015', border: '1px solid #333', color: '#fff', borderRadius: '4px', ...props.style }} />
);

const Label = ({ children }: { children: React.ReactNode }) => (
  <label style={{ display: 'block', fontSize: '12px', color: '#888', marginBottom: '4px' }}>{children}</label>
);

const Button = ({ children, variant = 'primary', ...props }: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: 'primary' | 'secondary' | 'danger' }) => {
  const bg = variant === 'primary' ? '#3dd6c6' : variant === 'danger' ? '#ef4444' : '#242e3a';
  const color = variant === 'primary' ? '#000' : '#fff';
  return (
    <button {...props} style={{ padding: '8px 16px', background: bg, color, border: 'none', borderRadius: '4px', cursor: 'pointer', fontWeight: 'bold', opacity: props.disabled ? 0.6 : 1, ...props.style }}>
      {children}
    </button>
  );
};

const Card = ({ children, active }: { children: React.ReactNode, active?: boolean }) => (
  <div style={{ background: '#141c24', padding: '1.5rem', borderRadius: '8px', border: active ? '1px solid #3dd6c6' : '1px solid #242e3a', marginBottom: '1rem' }}>
    {children}
  </div>
);

// --- 页面路由组件 ---

function DashboardPage({ data }: { data: DashboardData | null }) {
  if (!data) return <div style={{ color: '#888' }}>正在从 Rust Core 同步状态...</div>;
  return (
    <div>
      <h1 style={{ fontSize: '24px', marginBottom: '20px' }}>控制平面 (Control Plane)</h1>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '1rem' }}>
        <Card>
          <Label>MCP 网关地址</Label>
          <div style={{ fontSize: '20px', fontFamily: 'monospace', marginTop: '8px' }}>{data.gateway.endpoint}</div>
          <div style={{ marginTop: '12px', fontSize: '14px', color: data.gateway.online ? '#3dd6c6' : '#ef4444' }}>
            {data.gateway.online ? '● 在线并等待 AI 连接' : '○ 离线'}
          </div>
        </Card>
        <Card>
          <Label>已挂载项目数</Label>
          <div style={{ fontSize: '32px', fontWeight: 'bold', marginTop: '4px' }}>{data.projects.length}</div>
        </Card>
        <Card>
          <Label>默认执行器 (Executor)</Label>
          <div style={{ fontSize: '32px', fontWeight: 'bold', color: '#3dd6c6', marginTop: '4px', textTransform: 'capitalize' }}>{data.executor}</div>
        </Card>
      </div>
    </div>
  );
}

function ProjectsPage() {
  const [projects, setProjects] = useState<Project[]>([]);
  const [path, setPath] = useState('');
  const [name, setName] = useState('');

  const load = () => api.getProjects().then(setProjects).catch(console.error);
  useEffect(() => { load(); }, []);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.saveProject({ name, path, executor: "opencode" });
      setPath(''); setName(''); load();
    } catch (err: any) { alert(err.message); }
  };

  return (
    <div>
      <h1 style={{ fontSize: '24px', marginBottom: '20px' }}>本地工作区管理</h1>
      <Card>
        <h3 style={{ margin: '0 0 1rem 0', color: '#3dd6c6' }}>+ 挂载新项目</h3>
        <form onSubmit={handleAdd} style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end' }}>
          <div style={{ flex: 1 }}><Label>项目名称 (英文或数字)</Label><Input value={name} onChange={e => setName(e.target.value)} placeholder="例如: agentbridge-core" required /></div>
          <div style={{ flex: 2 }}><Label>本地绝对路径</Label><Input value={path} onChange={e => setPath(e.target.value)} placeholder="例如: D:\Projects\AgentBridge" required /></div>
          <Button type="submit">挂载项目</Button>
        </form>
      </Card>
      {projects.map(p => (
        <Card key={p.id} active={p.active}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <div style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: '18px', fontWeight: 'bold' }}>
                {p.name} {p.active && <span style={{ fontSize: '12px', background: 'rgba(61,214,198,0.2)', color: '#3dd6c6', padding: '2px 6px', borderRadius: '4px' }}>当前激活</span>}
              </div>
              <div style={{ color: '#888', fontFamily: 'monospace', marginTop: '8px' }}>{p.path}</div>
              <div style={{ display: 'flex', gap: '10px', marginTop: '10px' }}>
                {p.project_type.map(type => (
                  <span key={type} style={{ background: '#242e3a', color: '#ccc', padding: '2px 6px', borderRadius: '4px', fontSize: '12px' }}>{type}</span>
                ))}
                {p.git_repository && <span style={{ background: '#242e3a', color: '#ccc', padding: '2px 6px', borderRadius: '4px', fontSize: '12px' }}>Git 仓库</span>}
              </div>
            </div>
            <button onClick={() => { if(window.confirm('确定要移除此项目的挂载吗？(仅删除配置，不会影响本地文件)')) { api.deleteProject(p.id).then(load).catch(e=>alert(e)); } }} style={{ background: 'transparent', border: 'none', color: '#ef4444', cursor: 'pointer' }} title="移除挂载"><Trash2 /></button>
          </div>
        </Card>
      ))}
    </div>
  );
}

function ExecutorsPage() {
  const [executors, setExecutors] = useState<Executor[]>([]);
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');

  const load = () => api.getExecutors().then(setExecutors).catch(console.error);
  useEffect(() => { load(); }, []);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    try {
      await api.saveExecutor({ name, kind: "opencode", command, enabled: true });
      setName(''); setCommand(''); load();
    } catch (err: any) { alert(err.message); }
  };

  return (
    <div>
      <h1 style={{ fontSize: '24px', marginBottom: '20px' }}>执行器 (Executors) 配置</h1>
      <Card>
        <h3 style={{ margin: '0 0 1rem 0', color: '#3dd6c6' }}>+ 注册本地执行器</h3>
        <form onSubmit={handleAdd} style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end' }}>
          <div style={{ flex: 1 }}><Label>显示名称</Label><Input value={name} onChange={e => setName(e.target.value)} placeholder="例如: 本地 OpenCode" required /></div>
          <div style={{ flex: 2 }}><Label>终端命令或绝对路径</Label><Input value={command} onChange={e => setCommand(e.target.value)} placeholder="例如: opencode" required /></div>
          <Button type="submit">注册执行器</Button>
        </form>
      </Card>
      {executors.map(ex => (
        <Card key={ex.id}>
          <div style={{ display: 'flex', justifyContent: 'space-between' }}>
            <div>
              <div style={{ fontSize: '18px', fontWeight: 'bold' }}>{ex.name}</div>
              <div style={{ color: '#888', fontFamily: 'monospace', marginTop: '4px' }}>$ {ex.command}</div>
              <div style={{ display: 'flex', alignItems: 'center', gap: '6px', marginTop: '12px', fontSize: '14px', color: ex.available ? '#3dd6c6' : '#ef4444' }}>
                {ex.available ? <CheckCircle2 size={16}/> : <XCircle size={16}/>}
                {ex.status === 'available' ? '就绪' : ex.status === 'not_found' ? '未找到命令' : ex.status} {ex.version ? `(${ex.version})` : ''}
              </div>
            </div>
            <div style={{ display: 'flex', gap: '8px' }}>
              <Button variant="secondary" onClick={() => api.testExecutor(ex.id).then(load).catch(e=>alert(e))}>连通性测试</Button>
              {!ex.detected && <Button variant="danger" onClick={() => { if(window.confirm('确定删除此执行器吗？')) api.deleteExecutor(ex.id).then(load).catch(e=>alert(e)); }}>删除</Button>}
            </div>
          </div>
        </Card>
      ))}
    </div>
  );
}

function SettingsPage() {
  const [conn, setConn] = useState<ConnectionData | null>(null);
  const [proxy, setProxy] = useState<ProxyData | null>(null);

  useEffect(() => {
    api.getConnection().then(setConn).catch(console.error);
    api.getProxy().then(setProxy).catch(console.error);
  }, []);

  return (
    <div>
      <h1 style={{ fontSize: '24px', marginBottom: '20px' }}>系统设置</h1>
      {conn && (
        <Card>
          <h3 style={{ margin: '0 0 1rem 0' }}>网关监听配置</h3>
          <div style={{ color: '#888', fontSize: '14px', marginBottom: '1rem' }}>配置文件路径: {conn.config_path}</div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '1rem' }}>
            <div><Label>监听地址 (Host)</Label><Input disabled value={conn.host} /></div>
            <div><Label>监听端口 (Port)</Label><Input disabled value={conn.port.toString()} /></div>
          </div>
          <div style={{ marginTop: '1rem', color: '#ef4444', fontSize: '14px' }}>
            * 修改监听地址或端口需要重启 Rust 守护进程。请直接编辑上方路径中的配置文件。
          </div>
        </Card>
      )}

      {proxy && (
        <Card>
          <h3 style={{ margin: '0 0 1rem 0' }}>全局网络代理 (Proxy)</h3>
          <div style={{ display: 'flex', alignItems: 'center', gap: '1rem', marginBottom: '1rem' }}>
            <Label>启用代理</Label>
            <input type="checkbox" checked={proxy.enabled} onChange={async (e) => {
              const res = await api.saveProxy({ ...proxy, enabled: e.target.checked });
              setProxy(res);
            }} />
          </div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 2fr 1fr', gap: '1rem' }}>
            <div><Label>协议类型</Label><Input disabled value={proxy.kind.toUpperCase()} /></div>
            <div><Label>代理主机</Label><Input disabled value={proxy.host} /></div>
            <div><Label>代理端口</Label><Input disabled value={proxy.port.toString()} /></div>
          </div>
        </Card>
      )}
    </div>
  );
}

// --- App 主框架 ---

export default function App() {
  const [data, setData] = useState<DashboardData | null>(null);
  useEffect(() => { api.getDashboard().then(setData).catch(console.error); }, []);

  return (
    <div style={{ display: 'flex', height: '100vh', fontFamily: '-apple-system, sans-serif', background: '#0b1015', color: '#e7ecf1' }}>
      <aside style={{ width: '240px', background: '#141c24', borderRight: '1px solid #242e3a', display: 'flex', flexDirection: 'column' }}>
        <div style={{ padding: '24px 20px' }}>
          <h2 style={{ color: '#3dd6c6', margin: 0, display: 'flex', alignItems: 'center', gap: '10px' }}><Activity /> AgentBridge</h2>
        </div>
        <nav style={{ flex: 1, padding: '0 12px', display: 'flex', flexDirection: 'column', gap: '4px' }}>
          <NavLink to="/" style={({isActive}) => ({ color: isActive ? '#3dd6c6' : '#888', textDecoration: 'none', display: 'flex', gap: '12px', padding: '12px', background: isActive ? '#0b1015' : 'transparent', borderRadius: '8px', fontWeight: 500 })}><LayoutDashboard size={20}/> 仪表盘</NavLink>
          <NavLink to="/projects" style={({isActive}) => ({ color: isActive ? '#3dd6c6' : '#888', textDecoration: 'none', display: 'flex', gap: '12px', padding: '12px', background: isActive ? '#0b1015' : 'transparent', borderRadius: '8px', fontWeight: 500 })}><FolderGit2 size={20}/> 项目管理</NavLink>
          <NavLink to="/executors" style={({isActive}) => ({ color: isActive ? '#3dd6c6' : '#888', textDecoration: 'none', display: 'flex', gap: '12px', padding: '12px', background: isActive ? '#0b1015' : 'transparent', borderRadius: '8px', fontWeight: 500 })}><Cpu size={20}/> 执行器</NavLink>
          <NavLink to="/topology" style={({isActive}) => ({ color: isActive ? '#3dd6c6' : '#888', textDecoration: 'none', display: 'flex', gap: '12px', padding: '12px', background: isActive ? '#0b1015' : 'transparent', borderRadius: '8px', fontWeight: 500 })}><Play size={20}/> 节点拓扑</NavLink>
        </nav>
        <div style={{ padding: '12px' }}>
          <NavLink to="/settings" style={({isActive}) => ({ color: isActive ? '#3dd6c6' : '#888', textDecoration: 'none', display: 'flex', gap: '12px', padding: '12px', background: isActive ? '#0b1015' : 'transparent', borderRadius: '8px', fontWeight: 500 })}><Settings size={20}/> 设置</NavLink>
        </div>
      </aside>

      <main style={{ flex: 1, padding: '2.5rem', overflowY: 'auto' }}>
        <Routes>
          <Route path="/" element={<DashboardPage data={data} />} />
          <Route path="/projects" element={<ProjectsPage />} />
          <Route path="/executors" element={<ExecutorsPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/topology" element={<div style={{color:'#888'}}>等待 D3.js 节点拓扑集成...</div>} />
        </Routes>
      </main>
    </div>
  );
}

const root = createRoot(document.getElementById('root')!);
root.render(<BrowserRouter><App /></BrowserRouter>);