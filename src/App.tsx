import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import Identity from './components/Identity';
import GhostChat from './components/GhostChat';
import DeadDrop from './components/DeadDrop';
import './index.css';

type Tab = 'identity' | 'ghost' | 'deaddrop';

function App() {
  const [activeTab, setActiveTab] = useState<Tab>('identity');
  const [publicId, setPublicId] = useState<string>('');
  const [isInitialized, setIsInitialized] = useState(false);

  useEffect(() => {
    initializeIdentity();
  }, []);

  const initializeIdentity = async () => {
    try {
      console.log('Initializing identity...');
      const id = await invoke<string>('init_identity', {
        password: 'deaddrop-secure-2024',
      });
      console.log('Identity initialized:', id);
      setPublicId(id);
      setIsInitialized(true);
    } catch (error) {
      console.error('Failed to initialize identity:', error);
      alert('Failed to initialize identity: ' + error);
    }
  };

  return (
    <div style={{ display: 'flex', height: '100vh', width: '100vw' }}>
      {/* SIDEBAR */}
      <aside
        style={{
          width: '320px',
          background: '#000',
          borderRight: '4px solid #FFF',
          display: 'flex',
          flexDirection: 'column',
          padding: '40px 24px',
        }}
      >
        {/* LOGO */}
        <div style={{ marginBottom: '60px' }}>
          <img 
            src="/icons/logo-white.png" 
            alt="Control" 
            style={{ 
              width: '100%', 
              maxWidth: '180px',
              imageRendering: 'crisp-edges',
            }} 
          />
        </div>

        {/* MENU */}
        <nav style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <button
            className={`btn-bold ${activeTab === 'identity' ? 'active' : ''}`}
            onClick={() => setActiveTab('identity')}
            style={{ width: '100%', textAlign: 'left' }}
          >
            IDENTITY
          </button>

          <button
            className={`btn-bold ${activeTab === 'ghost' ? 'active' : ''}`}
            onClick={() => setActiveTab('ghost')}
            disabled={!isInitialized}
            style={{ width: '100%', textAlign: 'left' }}
          >
            GHOST MODE
          </button>

          <button
            className={`btn-bold ${activeTab === 'deaddrop' ? 'active' : ''}`}
            onClick={() => setActiveTab('deaddrop')}
            disabled={!isInitialized}
            style={{ width: '100%', textAlign: 'left' }}
          >
            DEAD DROP
          </button>
        </nav>

        {/* STATUS */}
        <div
          style={{
            marginTop: 'auto',
            paddingTop: '40px',
            borderTop: '4px solid #FFF',
          }}
        >
          <div style={{ fontSize: '14px', fontWeight: 700, marginBottom: '8px' }}>
            STATUS
          </div>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '8px',
            }}
          >
            <div
              className={isInitialized ? 'pulse-red' : ''}
              style={{
                width: '12px',
                height: '12px',
                background: isInitialized ? '#FF0000' : '#666',
                border: '2px solid #FFF',
              }}
            />
            <span style={{ fontSize: '12px', fontWeight: 500 }}>
              {isInitialized ? 'OPERATIONAL' : 'INITIALIZING'}
            </span>
          </div>
        </div>
      </aside>

      {/* CONTENT AREA */}
      <main
        style={{
          flex: 1,
          background: '#000',
          overflow: 'auto',
        }}
      >
        {activeTab === 'identity' && <Identity publicId={publicId} />}
        {activeTab === 'ghost' && <GhostChat publicId={publicId} />}
        {activeTab === 'deaddrop' && <DeadDrop />}
      </main>
    </div>
  );
}

export default App;
