/**
 * Web stub for react-native-vision-camera
 * Camera features are not available on web platform
 */

import React from 'react';
import { View, Text, StyleSheet } from 'react-native';

// Stub Camera component for web
export const Camera = React.forwardRef(function CameraStub(props, ref) {
  const { style, children } = props;
  return React.createElement(
    View,
    { style: [styles.camera, style] },
    React.createElement(Text, { style: styles.text }, 'Camera not available on web'),
    React.createElement(Text, { style: styles.subtext }, 'Use mobile device to scan QR codes'),
    children
  );
});

Camera.displayName = 'Camera';

// Add static method stubs
Camera.requestCameraPermission = async function() { return 'denied'; };
Camera.getCameraPermissionStatus = async function() { return 'denied'; };

// Hook stubs
export function useCameraDevice(type) {
  return null;
}

export function useCodeScanner(options) {
  return {};
}

const styles = StyleSheet.create({
  camera: {
    backgroundColor: '#1a1a2e',
    justifyContent: 'center',
    alignItems: 'center',
  },
  text: {
    color: '#fff',
    fontSize: 18,
    fontWeight: 'bold',
  },
  subtext: {
    color: '#aaa',
    fontSize: 14,
    marginTop: 8,
  },
});
