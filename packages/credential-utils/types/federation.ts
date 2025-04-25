// Federation-related types
export interface FederationManifest {
  federation_id: string;
  name: string;
  members: Record<string, FederationMemberRole>;
  quorum_rules: QuorumConfig;
  created: string;
  updated?: string;
  version: number;
  description?: string;
  health_metrics?: FederationHealthMetrics;
}

export interface FederationMemberRole {
  role: string;
  weight: number;
  voting_power?: number;
  can_veto?: boolean;
}

export interface QuorumConfig {
  policy_type: 'Majority' | 'Threshold' | 'Unanimous' | 'Weighted';
  min_participants: number;
  min_approvals: number;
  threshold_percentage?: number; // For Threshold and Weighted policies
  timeout_seconds?: number;
}

export interface FederationHealthMetrics {
  overall_health: number; // 0-100 score
  metrics: Record<string, number>;
  warnings: string[];
  recommendations: string[];
}

export interface TrustScoreResult {
  score: number; // 0-100 score
  status: 'High' | 'Medium' | 'Low';
  breakdown: {
    valid_signature: boolean;
    registered_member: boolean;
    quorum_threshold_met: boolean;
    sufficient_signer_weight: boolean;
    federation_health: number; // 0-100
    dag_ancestry_valid?: boolean;
  };
  summary: string;
  details: string[];
}

export interface FederationTrust {
  score: number;
  status: string;
  breakdown: {
    valid_signature: boolean;
    registered_member: boolean;
    quorum_threshold_met: boolean;
    sufficient_signer_weight: boolean;
    federation_health: number;
  };
  summary: string;
  details: string[];
} 