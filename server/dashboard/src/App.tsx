import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { Layout } from '@/components/layout/Layout';
import { Login } from '@/pages/Login';
import { Dashboard } from '@/pages/Dashboard';
import { Bots } from '@/pages/Bots';
import { Config } from '@/pages/Config';
import { Infrastructure } from '@/pages/Infrastructure';
import { Data } from '@/pages/Data';
import { Logs } from '@/pages/Logs';
import { Analytics } from '@/pages/Analytics';
import { api } from '@/api/client';
import { useStore } from '@/stores/useStore';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      staleTime: 5000,
    },
  },
});

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { user, setUser } = useStore();
  const [checking, setChecking] = useState(true);

  useEffect(() => {
    const token = api.getToken();
    if (token && !user) {
      // Try to validate token
      api.getMe()
        .then((response) => {
          setUser(response.user);
        })
        .catch(() => {
          api.setToken(null);
        })
        .finally(() => {
          setChecking(false);
        });
    } else {
      setChecking(false);
    }
  }, [user, setUser]);

  if (checking) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-[hsl(var(--background))]">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-[hsl(var(--primary))]" />
      </div>
    );
  }

  if (!api.getToken()) {
    return <Navigate to="/login" replace />;
  }

  return <>{children}</>;
}

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route
            path="/"
            element={
              <ProtectedRoute>
                <Layout />
              </ProtectedRoute>
            }
          >
            <Route index element={<Dashboard />} />
            <Route path="bots" element={<Bots />} />
            <Route path="analytics" element={<Analytics />} />
            <Route path="infrastructure" element={<Infrastructure />} />
            <Route path="config" element={<Config />} />
            <Route path="data" element={<Data />} />
            <Route path="logs" element={<Logs />} />
          </Route>
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

export default App;
