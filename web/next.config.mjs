/** @type {import('next').NextConfig} */
const API = process.env.TERMINAL_API_URL || 'http://127.0.0.1:8080';
const dev = process.env.NODE_ENV !== 'production';
const SCRIPT_SRC = `script-src 'self' 'unsafe-inline'${dev ? " 'unsafe-eval'" : ''}`;
export default {
  output: 'standalone',
  poweredByHeader: false,
  async rewrites() { return [{ source: '/api/:path*', destination: `${API}/api/:path*` }]; },
  async headers() {
    return [{ source: '/:path*', headers: [
      { key: 'X-Frame-Options', value: 'DENY' },
      { key: 'X-Content-Type-Options', value: 'nosniff' },
      { key: 'Referrer-Policy', value: 'strict-origin-when-cross-origin' },
      { key: 'Content-Security-Policy', value: `default-src 'self'; ${SCRIPT_SRC}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'` },
    ] }];
  },
};
