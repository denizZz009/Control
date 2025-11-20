import { useEffect, useRef } from 'react';
import QRCode from 'qrcode';

interface IdentityProps {
  publicId: string;
}

function Identity({ publicId }: IdentityProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    if (publicId && canvasRef.current) {
      QRCode.toCanvas(
        canvasRef.current,
        publicId,
        {
          width: 400,
          margin: 2,
          color: {
            dark: '#000000',
            light: '#FFFFFF',
          },
        },
        (error) => {
          if (error) console.error('QR Code generation failed:', error);
        }
      );
    }
  }, [publicId]);

  const copyToClipboard = () => {
    navigator.clipboard.writeText(publicId);
  };

  return (
    <div style={{ padding: '60px 80px' }}>
      {/* HEADER */}
      <div style={{ marginBottom: '60px' }}>
        <h1 className="text-huge">
          IDENTITY <span style={{ color: '#FF0000' }}>//</span> CONTROL
        </h1>
        <p style={{ fontSize: '18px', fontWeight: 500, marginTop: '16px', opacity: 0.7 }}>
          YOUR CRYPTOGRAPHIC FINGERPRINT
        </p>
      </div>

      {/* PUBLIC ID DISPLAY */}
      <div style={{ marginBottom: '60px' }}>
        <div
          style={{
            fontSize: '14px',
            fontWeight: 900,
            marginBottom: '12px',
            letterSpacing: '0.1em',
          }}
        >
          PUBLIC KEY
        </div>
        <div
          className="box-bold"
          style={{
            padding: '24px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            gap: '24px',
          }}
        >
          <code
            className="mono"
            style={{
              fontSize: '16px',
              wordBreak: 'break-all',
              flex: 1,
            }}
          >
            {publicId || 'GENERATING...'}
          </code>
          <button
            className="btn-bold"
            onClick={copyToClipboard}
            disabled={!publicId}
            style={{ flexShrink: 0 }}
          >
            COPY
          </button>
        </div>
      </div>

      {/* QR CODE */}
      <div>
        <div
          style={{
            fontSize: '14px',
            fontWeight: 900,
            marginBottom: '12px',
            letterSpacing: '0.1em',
          }}
        >
          QR CODE
        </div>
        <div
          className="box-bold inverted"
          style={{
            display: 'inline-block',
            padding: '40px',
          }}
        >
          <canvas ref={canvasRef} />
        </div>
        <p
          style={{
            fontSize: '14px',
            fontWeight: 500,
            marginTop: '16px',
            opacity: 0.6,
          }}
        >
          SCAN TO SHARE YOUR PUBLIC KEY
        </p>
      </div>

      {/* SECURITY NOTICE */}
      <div
        className="box-bold danger"
        style={{
          marginTop: '60px',
          padding: '32px',
          background: 'rgba(255, 0, 0, 0.05)',
        }}
      >
        <div
          style={{
            fontSize: '18px',
            fontWeight: 900,
            marginBottom: '12px',
            color: '#FF0000',
          }}
        >
          ⚠ SECURITY NOTICE
        </div>
        <p style={{ fontSize: '14px', fontWeight: 500, lineHeight: 1.6 }}>
          Your private key is encrypted and stored locally. Never share your password.
          This public key can be safely distributed to establish secure communication
          channels.
        </p>
      </div>
    </div>
  );
}

export default Identity;
