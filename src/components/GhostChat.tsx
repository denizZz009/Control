import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { listen } from '@tauri-apps/api/event';

interface Message {
  id: string;
  from: string;
  content: string;
  timestamp: number;
  isOutgoing: boolean;
}

interface GhostChatProps {
  publicId: string;
}

function GhostChat({ publicId }: GhostChatProps) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [targetKey, setTargetKey] = useState('');
  const [messageContent, setMessageContent] = useState('');
  const [isGhostModeActive, setIsGhostModeActive] = useState(false);
  const [isSending, setIsSending] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // Listen for incoming messages
    const unlistenMsg = listen<any>('ghost_msg', (event) => {
      const msg = event.payload;
      setMessages((prev) => [
        ...prev,
        {
          id: msg.id || Date.now().toString(),
          from: msg.from,
          content: msg.content,
          timestamp: msg.timestamp,
          isOutgoing: false,
        },
      ]);
    });

    // Listen for delivery confirmations
    const unlistenDelivered = listen<any>('msg_delivered', (event) => {
      console.log('Message delivered:', event.payload);
    });

    return () => {
      unlistenMsg.then((fn) => fn());
      unlistenDelivered.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const startGhostMode = async () => {
    try {
      await invoke('start_ghost_mode');
      setIsGhostModeActive(true);
    } catch (error) {
      console.error('Failed to start Ghost Mode:', error);
      alert('Failed to start Ghost Mode: ' + error);
    }
  };

  const sendMessage = async () => {
    if (!messageContent.trim() || !targetKey.trim()) return;

    setIsSending(true);
    try {
      const messageId = await invoke<string>('send_ghost_message', {
        targetPublicKey: targetKey,
        content: messageContent,
      });

      setMessages((prev) => [
        ...prev,
        {
          id: messageId,
          from: publicId,
          content: messageContent,
          timestamp: Date.now() / 1000,
          isOutgoing: true,
        },
      ]);

      setMessageContent('');
    } catch (error) {
      console.error('Failed to send message:', error);
      alert('Failed to send message: ' + error);
    } finally {
      setIsSending(false);
    }
  };

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      {/* HEADER */}
      <div
        style={{
          padding: '40px 60px',
          borderBottom: '4px solid #FFF',
        }}
      >
        <h1 className="text-big">
          GHOST MODE <span style={{ color: '#FF0000' }}>//</span> CONTROL
        </h1>
        <div style={{ marginTop: '24px', display: 'flex', gap: '16px', alignItems: 'center' }}>
          {!isGhostModeActive ? (
            <button className="btn-bold" onClick={startGhostMode}>
              ACTIVATE GHOST MODE
            </button>
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
              <div
                className="pulse-red"
                style={{
                  width: '16px',
                  height: '16px',
                  background: '#FF0000',
                  border: '2px solid #FFF',
                }}
              />
              <span style={{ fontSize: '16px', fontWeight: 700 }}>GHOST MODE ACTIVE</span>
            </div>
          )}
        </div>
      </div>

      {/* TARGET INPUT */}
      {isGhostModeActive && (
        <div
          style={{
            padding: '24px 60px',
            borderBottom: '4px solid #FFF',
            background: 'rgba(255, 255, 255, 0.02)',
          }}
        >
          <div style={{ fontSize: '12px', fontWeight: 900, marginBottom: '8px' }}>
            TARGET PUBLIC KEY
          </div>
          <input
            type="text"
            className="input-bold mono"
            placeholder="PASTE RECIPIENT'S PUBLIC KEY..."
            value={targetKey}
            onChange={(e) => setTargetKey(e.target.value)}
            style={{ fontSize: '14px' }}
          />
        </div>
      )}

      {/* MESSAGES FEED */}
      <div
        style={{
          flex: 1,
          padding: '40px 60px',
          overflowY: 'auto',
          display: 'flex',
          flexDirection: 'column',
          gap: '24px',
        }}
      >
        {messages.length === 0 ? (
          <div
            style={{
              textAlign: 'center',
              padding: '60px 20px',
              opacity: 0.3,
            }}
          >
            <div style={{ fontSize: '48px', marginBottom: '16px' }}>📡</div>
            <div style={{ fontSize: '18px', fontWeight: 700 }}>NO TRANSMISSIONS</div>
            <div style={{ fontSize: '14px', fontWeight: 500, marginTop: '8px' }}>
              AWAITING SECURE COMMUNICATIONS
            </div>
          </div>
        ) : (
          messages.map((msg) => (
            <div
              key={msg.id}
              className="slide-in"
              style={{
                display: 'flex',
                justifyContent: msg.isOutgoing ? 'flex-end' : 'flex-start',
              }}
            >
              <div
                className={`box-bold ${msg.isOutgoing ? 'inverted' : ''}`}
                style={{
                  maxWidth: '70%',
                  padding: '20px 24px',
                }}
              >
                <div
                  style={{
                    fontSize: '11px',
                    fontWeight: 900,
                    marginBottom: '8px',
                    opacity: 0.6,
                  }}
                >
                  {msg.isOutgoing ? 'OUTGOING' : 'INCOMING'} //
                  {new Date(msg.timestamp * 1000).toLocaleTimeString()}
                </div>
                <div style={{ fontSize: '16px', fontWeight: 700, lineHeight: 1.5 }}>
                  {msg.content}
                </div>
                <div
                  className="mono"
                  style={{
                    fontSize: '10px',
                    marginTop: '12px',
                    opacity: 0.4,
                    wordBreak: 'break-all',
                  }}
                >
                  {msg.from.substring(0, 16)}...
                </div>
              </div>
            </div>
          ))
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* MESSAGE INPUT */}
      {isGhostModeActive && (
        <div
          style={{
            padding: '24px 60px',
            borderTop: '4px solid #FFF',
            background: '#000',
          }}
        >
          <div style={{ display: 'flex', gap: '16px' }}>
            <input
              type="text"
              className="input-bold"
              placeholder="TRANSMIT MESSAGE..."
              value={messageContent}
              onChange={(e) => setMessageContent(e.target.value)}
              onKeyPress={(e) => e.key === 'Enter' && sendMessage()}
              disabled={!targetKey || isSending}
              style={{ flex: 1 }}
            />
            <button
              className="btn-bold"
              onClick={sendMessage}
              disabled={!messageContent.trim() || !targetKey || isSending}
              style={{ minWidth: '160px' }}
            >
              {isSending ? 'SENDING...' : 'TRANSMIT'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default GhostChat;
