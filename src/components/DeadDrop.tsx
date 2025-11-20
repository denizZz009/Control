import { useState, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/tauri';
import { open } from '@tauri-apps/api/dialog';

interface DeadDropCreated {
  cid: string;
  shards: string[];
}

function DeadDrop() {
  const [isDragging, setIsDragging] = useState(false);
  const [isProcessing, setIsProcessing] = useState(false);
  const [result, setResult] = useState<DeadDropCreated | null>(null);
  const [threshold, setThreshold] = useState(2);
  const [totalShards, setTotalShards] = useState(3);
  const [ipfsStatus, setIpfsStatus] = useState<string>('');

  const testIpfs = async () => {
    try {
      const status = await invoke<string>('test_ipfs');
      setIpfsStatus(status);
      alert('✓ ' + status);
    } catch (error) {
      setIpfsStatus('IPFS not running: ' + error);
      alert('✗ IPFS not running. Please start IPFS daemon:\n\nipfs daemon');
    }
  };

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(true);
  }, []);

  const handleDragLeave = useCallback(() => {
    setIsDragging(false);
  }, []);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      setIsDragging(false);

      // Note: File.path is not available in browser
      // Use file picker instead for Tauri
      alert('Please use the file picker (click the drop zone)');
    },
    [threshold, totalShards]
  );

  const handleFileSelect = async () => {
    const selected = await open({
      multiple: false,
      title: 'SELECT PAYLOAD',
    });

    if (selected && typeof selected === 'string') {
      await processFile(selected);
    }
  };

  const processFile = async (filePath: string) => {
    setIsProcessing(true);
    setResult(null);

    try {
      const dropResult = await invoke<DeadDropCreated>('create_drop', {
        filePath,
        threshold,
        totalShards,
      });

      setResult(dropResult);
    } catch (error) {
      console.error('Failed to create dead drop:', error);
      alert('Failed to create dead drop: ' + error);
    } finally {
      setIsProcessing(false);
    }
  };

  const copyShard = (shard: string) => {
    navigator.clipboard.writeText(shard);
  };

  const copyCID = () => {
    if (result) {
      navigator.clipboard.writeText(result.cid);
    }
  };

  return (
    <div style={{ height: '100%', display: 'flex', flexDirection: 'column' }}>
      {/* HEADER */}
      <div style={{ padding: '40px 60px', borderBottom: '4px solid #FFF' }}>
        <h1 className="text-big">
          DEAD DROP <span style={{ color: '#FF0000' }}>//</span> CONTROL
        </h1>
        <p style={{ fontSize: '16px', fontWeight: 500, marginTop: '12px', opacity: 0.7 }}>
          ENCRYPT FILES • SPLIT KEYS • DISTRIBUTE SHARDS
        </p>
      </div>

      {/* CONFIGURATION */}
      <div
        style={{
          padding: '24px 60px',
          borderBottom: '4px solid #FFF',
          background: 'rgba(255, 255, 255, 0.02)',
        }}
      >
        <div style={{ display: 'flex', gap: '40px', alignItems: 'flex-end' }}>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: '12px', fontWeight: 900, marginBottom: '8px' }}>
              THRESHOLD (MIN SHARDS)
            </div>
            <input
              type="number"
              className="input-bold"
              value={threshold}
              onChange={(e) => setThreshold(parseInt(e.target.value) || 2)}
              min={2}
              max={totalShards}
              style={{ fontSize: '18px', textAlign: 'center' }}
            />
          </div>
          <div style={{ flex: 1 }}>
            <div style={{ fontSize: '12px', fontWeight: 900, marginBottom: '8px' }}>
              TOTAL SHARDS
            </div>
            <input
              type="number"
              className="input-bold"
              value={totalShards}
              onChange={(e) => setTotalShards(parseInt(e.target.value) || 3)}
              min={threshold}
              max={10}
              style={{ fontSize: '18px', textAlign: 'center' }}
            />
          </div>
          <div>
            <button className="btn-bold" onClick={testIpfs} style={{ fontSize: '14px' }}>
              TEST IPFS
            </button>
          </div>
        </div>
        {ipfsStatus && (
          <div
            style={{
              marginTop: '16px',
              padding: '12px',
              background: ipfsStatus.includes('not running') ? 'rgba(255,0,0,0.1)' : 'rgba(0,255,0,0.1)',
              border: `2px solid ${ipfsStatus.includes('not running') ? '#FF0000' : '#00FF00'}`,
              fontSize: '12px',
              fontWeight: 700,
            }}
          >
            {ipfsStatus}
          </div>
        )}
      </div>

      {/* DROP ZONE */}
      {!result && (
        <div
          style={{
            flex: 1,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            padding: '60px',
          }}
        >
          <div
            className={`box-bold ${isDragging ? 'inverted' : ''}`}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
            style={{
              width: '100%',
              maxWidth: '800px',
              height: '500px',
              display: 'flex',
              flexDirection: 'column',
              alignItems: 'center',
              justifyContent: 'center',
              cursor: 'pointer',
              transition: 'all 0.2s ease',
              background: isDragging ? '#FFF' : '#000',
              color: isDragging ? '#000' : '#FFF',
            }}
            onClick={!isProcessing ? handleFileSelect : undefined}
          >
            {isProcessing ? (
              <>
                <div style={{ fontSize: '64px', marginBottom: '24px' }}>⚙️</div>
                <div className="text-big">PROCESSING...</div>
                <div style={{ fontSize: '16px', fontWeight: 500, marginTop: '16px' }}>
                  ENCRYPTING • UPLOADING • SPLITTING
                </div>
              </>
            ) : (
              <>
                <div style={{ fontSize: '96px', marginBottom: '24px' }}>📦</div>
                <div className="text-huge" style={{ textAlign: 'center' }}>
                  DROP
                  <br />
                  PAYLOAD
                  <br />
                  HERE
                </div>
                <div
                  style={{
                    fontSize: '18px',
                    fontWeight: 500,
                    marginTop: '32px',
                    opacity: 0.6,
                  }}
                >
                  OR CLICK TO SELECT FILE
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* RESULT */}
      {result && (
        <div style={{ flex: 1, padding: '60px', overflowY: 'auto' }}>
          {/* CID */}
          <div style={{ marginBottom: '40px' }}>
            <div
              className="box-bold danger"
              style={{
                padding: '32px',
                background: 'rgba(255, 0, 0, 0.1)',
              }}
            >
              <div
                style={{
                  fontSize: '14px',
                  fontWeight: 900,
                  marginBottom: '16px',
                  color: '#FF0000',
                }}
              >
                ✓ PAYLOAD ENCRYPTED & UPLOADED
              </div>
              <div style={{ fontSize: '12px', fontWeight: 900, marginBottom: '8px' }}>
                IPFS CID
              </div>
              <div style={{ display: 'flex', gap: '16px', alignItems: 'center' }}>
                <code
                  className="mono"
                  style={{
                    fontSize: '16px',
                    wordBreak: 'break-all',
                    flex: 1,
                  }}
                >
                  {result.cid}
                </code>
                <button className="btn-bold" onClick={copyCID}>
                  COPY
                </button>
              </div>
            </div>
          </div>

          {/* SHARDS */}
          <div>
            <div
              style={{
                fontSize: '18px',
                fontWeight: 900,
                marginBottom: '24px',
              }}
            >
              KEY SHARDS ({threshold} OF {totalShards} REQUIRED)
            </div>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
              {result.shards.map((shard, index) => (
                <div key={index} className="box-bold" style={{ padding: '20px' }}>
                  <div
                    style={{
                      fontSize: '12px',
                      fontWeight: 900,
                      marginBottom: '12px',
                    }}
                  >
                    SHARD {index + 1}
                  </div>
                  <div style={{ display: 'flex', gap: '16px', alignItems: 'center' }}>
                    <code
                      className="mono"
                      style={{
                        fontSize: '12px',
                        wordBreak: 'break-all',
                        flex: 1,
                        opacity: 0.8,
                      }}
                    >
                      {shard}
                    </code>
                    <button
                      className="btn-bold"
                      onClick={() => copyShard(shard)}
                      style={{ fontSize: '14px', padding: '12px 24px' }}
                    >
                      COPY
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* RESET */}
          <div style={{ marginTop: '40px', textAlign: 'center' }}>
            <button
              className="btn-bold"
              onClick={() => setResult(null)}
              style={{ minWidth: '240px' }}
            >
              CREATE NEW DROP
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

export default DeadDrop;
