// web/src/components/Layout.tsx
import { NavLink, Outlet } from 'react-router-dom';
import { Activity } from 'lucide-react';
import { navigation } from '../navigation';
import { theme } from '../theme';

export function Layout() {
  return (
    <div className="ab-shell">
      <aside className="ab-sidebar">
        <div style={{ padding: '22px 20px 16px' }}>
          <div
            style={{
              color: theme.accent,
              display: 'flex',
              alignItems: 'center',
              gap: 10,
              fontSize: 18,
              fontWeight: 700,
            }}
          >
            <Activity size={22} /> <span className="ab-brand-text">AgentBridge</span>
          </div>
          <div
            className="ab-brand-text"
            style={{ color: theme.faint, fontSize: 11, marginTop: 4 }}
          >
            Control Plane
          </div>
        </div>

        <nav style={{ flex: 1, overflowY: 'auto', padding: '0 12px 12px' }}>
          {navigation.map((section) => (
            <div key={section.title} style={{ marginBottom: 14 }}>
              <div
                className="ab-section-title"
                style={{
                  fontSize: 11,
                  textTransform: 'uppercase',
                  letterSpacing: 1,
                  color: theme.faint,
                  padding: '0 12px',
                  marginBottom: 6,
                }}
              >
                {section.title}
              </div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                {section.items.map((item) => {
                  const Icon = item.icon;
                  return (
                    <NavLink
                      key={item.to}
                      to={item.to}
                      end={item.to === '/'}
                      className="ab-nav-link"
                      style={({ isActive }) => ({
                        display: 'flex',
                        alignItems: 'center',
                        gap: 12,
                        padding: '9px 12px',
                        borderRadius: 8,
                        textDecoration: 'none',
                        fontWeight: 500,
                        fontSize: 14,
                        color: isActive ? theme.accent : theme.muted,
                        background: isActive ? theme.bg : 'transparent',
                      })}
                    >
                      <Icon size={18} />
                      <span className="ab-nav-label">{item.label}</span>
                      {!item.implemented && (
                        <span
                          className="ab-nav-pending"
                          style={{
                            marginLeft: 'auto',
                            fontSize: 10,
                            color: theme.faint,
                            border: `1px solid ${theme.border}`,
                            borderRadius: 4,
                            padding: '1px 5px',
                          }}
                        >
                          待开发
                        </span>
                      )}
                    </NavLink>
                  );
                })}
              </div>
            </div>
          ))}
        </nav>
      </aside>

      <main className="ab-main">
        <Outlet />
      </main>
    </div>
  );
}
