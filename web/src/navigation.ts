// web/src/navigation.ts
import type { LucideIcon } from 'lucide-react';
import {
  Bot,
  Boxes,
  Cpu,
  FolderGit2,
  History,
  LayoutDashboard,
  ListChecks,
  Network,
  Settings,
  ShieldCheck,
  Sparkles,
  Workflow,
} from 'lucide-react';

export interface NavItem {
  to: string;
  label: string;
  icon: LucideIcon;
  /** false => placeholder route for a module that is not implemented yet. */
  implemented: boolean;
}

export interface NavSection {
  title: string;
  items: NavItem[];
}

export const navigation: NavSection[] = [
  {
    title: '概览',
    items: [
      { to: '/', label: '仪表盘', icon: LayoutDashboard, implemented: true },
    ],
  },
  {
    title: '资源管理',
    items: [
      { to: '/projects', label: '项目', icon: FolderGit2, implemented: true },
      { to: '/executors', label: '执行器', icon: Cpu, implemented: true },
      { to: '/providers', label: 'Provider', icon: Boxes, implemented: false },
      { to: '/proxy', label: '代理', icon: Network, implemented: true },
    ],
  },
  {
    title: '任务',
    items: [
      { to: '/tasks', label: '当前任务', icon: ListChecks, implemented: false },
      { to: '/tasks/history', label: '任务历史', icon: History, implemented: false },
    ],
  },
  {
    title: 'Agent',
    items: [
      { to: '/agent', label: 'Agent', icon: Bot, implemented: true },
      { to: '/agent/context', label: 'Agent Context', icon: Workflow, implemented: true },
      { to: '/skills', label: '技能', icon: Sparkles, implemented: true },
    ],
  },
  {
    title: '系统',
    items: [
      { to: '/security', label: '安全 / OAuth', icon: ShieldCheck, implemented: false },
      { to: '/settings', label: '设置', icon: Settings, implemented: true },
    ],
  },
];
