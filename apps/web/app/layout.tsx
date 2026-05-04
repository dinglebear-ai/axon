import './styles.css';
import type { Metadata } from 'next';

export const metadata: Metadata = {
  title: 'Axon Admin',
  description: 'Axon setup and operations panel'
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
