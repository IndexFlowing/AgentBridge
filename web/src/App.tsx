// web/src/App.tsx
import { Route, Routes } from 'react-router-dom';
import { Layout } from './components/Layout';
import { DashboardPage } from './pages/DashboardPage';
import { ProjectsPage } from './pages/ProjectsPage';
import { ExecutorsPage } from './pages/ExecutorsPage';
import { SettingsPage } from './pages/SettingsPage';
import { PlaceholderPage } from './pages/PlaceholderPage';

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/projects" element={<ProjectsPage />} />
        <Route path="/executors" element={<ExecutorsPage />} />
        <Route path="/settings" element={<SettingsPage />} />

        <Route
          path="/providers"
          element={
            <PlaceholderPage
              title="Provider"
              description="AI Provider 与模型管理"
            />
          }
        />
        <Route
          path="/proxy"
          element={
            <PlaceholderPage title="代理" description="多代理与凭据管理" />
          }
        />
        <Route
          path="/tasks"
          element={
            <PlaceholderPage title="当前任务" description="任务实时监控" />
          }
        />
        <Route
          path="/tasks/history"
          element={
            <PlaceholderPage title="任务历史" description="历史任务与审计" />
          }
        />
        <Route
          path="/skills"
          element={<PlaceholderPage title="技能" description="Skill Manager" />}
        />
        <Route
          path="/security"
          element={
            <PlaceholderPage
              title="安全 / OAuth"
              description="认证、OAuth 与访问控制"
            />
          }
        />

        <Route
          path="*"
          element={
            <PlaceholderPage
              title="页面未找到"
              description="请从左侧导航选择可用模块"
            />
          }
        />
      </Route>
    </Routes>
  );
}
