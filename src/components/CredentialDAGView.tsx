import React, { useEffect, useRef, useState, useMemo } from 'react';
import * as d3 from 'd3';
import { WalletCredential } from '../../packages/credential-utils/types';
import { extractCredentialLineage } from '../../packages/credential-utils/utils/federationSignature';
import { validateFederationReport } from '../../packages/credential-utils/utils/quorumValidation';
import { FederationManifest } from '../../packages/credential-utils/types/federation';
import { tooltipStyles } from '../components/styles';

// Node interface for D3 force graph
interface Node extends d3.SimulationNodeDatum {
  id: string;
  type: string;
  label: string;
  date: string;
  proposalId?: string;
  threadId?: string;
  federationId?: string;
  radius: number;
  color: string;
  // Quorum validation fields
  isSignerNode?: boolean;
  signerDid?: string;  // Add signerDid to identify the signer
  isFederationReport?: boolean;
  quorumValidation?: {
    isSatisfied: boolean;
    policy: string;
    signers: {
      did: string;
      role: string;
      weight: number;
    }[];
    requiredApprovals: number;
    actualApprovals: number;
    requiredThreshold?: number;
    actualThreshold?: number;
    totalWeight?: number;
  };
}

// Link interface for D3 force graph
interface Link extends d3.SimulationLinkDatum<Node> {
  source: string | Node;
  target: string | Node;
  type: string;
  // Quorum validation fields
  isSignerLink?: boolean;
  signerWeight?: number;
}

// Props for the CredentialDAGView component
interface CredentialDAGViewProps {
  credentials: WalletCredential[];
  selectedCredentialId?: string;
  onCredentialSelect?: (id: string) => void;
  onThreadSelect?: (threadId: string) => void;
  onSignerSelect?: (signerDid: string, federationId: string) => void; // Add callback for signer selection
  width?: number;
  height?: number;
  showLabels?: boolean;
  groupByThread?: boolean;
  highlightSelected?: boolean;
  // Federation manifest for quorum validation
  federationManifests?: Record<string, FederationManifest>;
  // Display options for quorum visualization
  showSignerNodes?: boolean;
  showMissingSigners?: boolean;
}

/**
 * Component for visualizing credential lineage as a directed graph
 * Enhanced with quorum validation visualization
 */
export const CredentialDAGView: React.FC<CredentialDAGViewProps> = ({
  credentials,
  selectedCredentialId,
  onCredentialSelect,
  onThreadSelect,
  onSignerSelect,
  width = 800,
  height = 600,
  showLabels = true,
  groupByThread = false,
  highlightSelected = true,
  federationManifests = {},
  showSignerNodes = true,
  showMissingSigners = false,
}) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [hoveredNode, setHoveredNode] = useState<Node | null>(null);
  const [quorumValidationResults, setQuorumValidationResults] = useState<Record<string, any>>({});

  // Validate federation reports to get quorum information
  useEffect(() => {
    const validateFederationReports = async () => {
      const results: Record<string, any> = {};
      
      // Find all federation report credentials
      const federationReports = credentials.filter(cred => 
        cred.type.includes('FederationReport') || 
        (cred.metadata?.federationMetadata && 
          (cred as any).multiSignatureProof?.signatures?.length > 0)
      );
      
      // Validate each federation report
      for (const report of federationReports) {
        const federationId = report.metadata?.federation?.id || 
                            report.metadata?.federationMetadata?.federation_id ||
                            report.metadata?.agoranet?.federation_id;
        
        if (federationId && federationManifests[federationId]) {
          try {
            const manifest = federationManifests[federationId];
            const validationResult = await validateFederationReport(report as any, manifest);
            results[report.id] = validationResult;
          } catch (error) {
            console.error(`Error validating report ${report.id}:`, error);
            // Create a default failed validation result
            results[report.id] = {
              isValid: false,
              signers: [],
              quorumAnalysis: {
                requiredParticipants: 0,
                actualParticipants: 0,
                requiredApprovals: 0,
                actualApprovals: 0,
                totalWeight: 0,
                isSatisfied: false,
              },
              errors: [`Error validating report: ${error}`],
            };
          }
        }
      }
      
      setQuorumValidationResults(results);
    };
    
    if (Object.keys(federationManifests).length > 0) {
      validateFederationReports();
    }
  }, [credentials, federationManifests]);

  // Transform credentials to node and link data
  useEffect(() => {
    if (!credentials || credentials.length === 0) {
      setNodes([]);
      setLinks([]);
      return;
    }

    // Extract credential lineage relationships
    const lineage = extractCredentialLineage(credentials);
    
    // Create nodes from credentials
    let newNodes: Node[] = credentials.map(cred => {
      // Determine node type and properties
      const nodeType = cred.type;
      const proposalId = cred.credentialSubject.proposalId;
      const threadId = cred.metadata?.agoranet?.threadId;
      const federationId = cred.metadata?.federation?.id || 
                         cred.metadata?.federationMetadata?.federation_id ||
                         cred.metadata?.agoranet?.federation_id;
      
      // Get a label for the node
      const label = getNodeLabel(cred);
      
      // Determine if this is a federation report
      const isFederationReport = 
        Array.isArray(cred.type) && cred.type.includes('FederationReport') || 
        (cred.metadata?.federationMetadata && 
          (cred as any).multiSignatureProof?.signatures?.length > 0);
      
      // Set quorum validation info if available
      let quorumValidation = undefined;
      if (isFederationReport && quorumValidationResults[cred.id]) {
        const result = quorumValidationResults[cred.id];
        quorumValidation = {
          isSatisfied: result.isValid,
          policy: federationManifests[federationId]?.quorum_rules.policy_type || 'Unknown',
          signers: result.signers,
          requiredApprovals: result.quorumAnalysis.requiredApprovals,
          actualApprovals: result.quorumAnalysis.actualApprovals,
          requiredThreshold: result.quorumAnalysis.requiredThreshold,
          actualThreshold: result.quorumAnalysis.actualThreshold,
          totalWeight: result.quorumAnalysis.totalWeight,
        };
      }
      
      // Get color based on credential type
      const baseColor = getNodeColor(cred.type);
      
      // Adjust color based on quorum validation if it's a federation report
      let color = baseColor;
      if (isFederationReport && quorumValidation) {
        color = quorumValidation.isSatisfied ? '#4CAF50' : // Green for satisfied
                quorumValidation.signers.length > 0 ? '#FFC107' : // Yellow for partial
                '#F44336'; // Red for invalid
      }
      
      // Adjust size based on importance
      const radius = getNodeRadius(cred.type);
      
      return {
        id: cred.id,
        type: nodeType,
        label,
        date: cred.issuanceDate,
        proposalId,
        threadId,
        federationId,
        radius,
        color,
        isFederationReport,
        quorumValidation,
        signerDid: cred.credentialSubject.signerDid,
      };
    });
    
    // Create links from lineage relationships
    let newLinks: Link[] = [];
    
    Object.entries(lineage).forEach(([childId, parentIds]) => {
      parentIds.forEach(parentId => {
        // Check that both nodes exist in our dataset
        if (credentials.some(c => c.id === parentId) && credentials.some(c => c.id === childId)) {
          // Determine link type
          const child = credentials.find(c => c.id === childId);
          const parent = credentials.find(c => c.id === parentId);
          const linkType = child && parent ? getLinkType(parent.type, child.type) : 'default';
          
          newLinks.push({
            source: parentId,
            target: childId,
            type: linkType
          });
        }
      });
    });
    
    // Add signer nodes and links for federation reports if enabled
    if (showSignerNodes) {
      const signerNodes: Node[] = [];
      const signerLinks: Link[] = [];
      
      // For each federation report with quorum validation
      newNodes.forEach(node => {
        if (node.isFederationReport && node.quorumValidation && node.quorumValidation.signers.length > 0) {
          // Add nodes for each signer
          node.quorumValidation.signers.forEach(signer => {
            const signerNodeId = `signer-${node.id}-${signer.did}`;
            
            // Create signer node
            signerNodes.push({
              id: signerNodeId,
              type: 'signer',
              label: `${signer.role} (${signer.did.split(':').pop()})`,
              date: '',
              radius: 5 + (signer.weight * 2), // Size based on weight
              color: '#64B5F6', // Blue for signers
              isSignerNode: true,
              signerDid: signer.did,
            });
            
            // Create link from signer to report
            signerLinks.push({
              source: signerNodeId,
              target: node.id,
              type: 'signature',
              isSignerLink: true,
              signerWeight: signer.weight,
            });
          });
          
          // Add missing signer nodes if enabled
          if (showMissingSigners && !node.quorumValidation.isSatisfied && node.federationId) {
            const manifest = federationManifests[node.federationId];
            if (manifest) {
              // Get all members from manifest
              const allMembers = Object.entries(manifest.members);
              
              // Get DIDs of signers who already signed
              const signerDids = node.quorumValidation.signers.map(s => s.did);
              
              // Find members who didn't sign
              const missingMembers = allMembers.filter(([did]) => !signerDids.includes(did));
              
              // Add nodes for missing signers
              missingMembers.forEach(([did, role]) => {
                const missingSignerNodeId = `missing-signer-${did}-${node.id}`;
                
                // Create missing signer node
                signerNodes.push({
                  id: missingSignerNodeId,
                  type: 'missing-signer',
                  label: `Missing: ${role.role} (${did.split(':').pop()})`,
                  date: '',
                  radius: 5 + (role.weight * 2), // Size based on weight
                  color: '#BDBDBD', // Gray for missing signers
                  isSignerNode: true,
                  signerDid: did,
                });
                
                // Create dashed link from missing signer to report
                signerLinks.push({
                  source: missingSignerNodeId,
                  target: node.id,
                  type: 'missing-signature',
                  isSignerLink: true,
                  signerWeight: role.weight,
                });
              });
            }
          }
        }
      });
      
      // Add signer nodes and links to the graph
      newNodes = [...newNodes, ...signerNodes];
      newLinks = [...newLinks, ...signerLinks];
    }
    
    setNodes(newNodes);
    setLinks(newLinks);
  }, [credentials, quorumValidationResults, federationManifests, showSignerNodes, showMissingSigners]);

  // Create D3 force graph when nodes or links change
  useEffect(() => {
    if (!svgRef.current || nodes.length === 0) return;
    
    // Clear previous graph
    d3.select(svgRef.current).selectAll('*').remove();
    
    // Create svg container
    const svg = d3.select(svgRef.current)
      .attr('width', width)
      .attr('height', height)
      .attr('viewBox', [0, 0, width, height]);
    
    // Create link lines
    const linkElements = svg.append('g')
      .attr('stroke', '#999')
      .attr('stroke-opacity', 0.6)
      .selectAll('line')
      .data(links)
      .join('line')
      .attr('stroke-width', d => d.isSignerLink ? 1 + (d.signerWeight || 0) : 2)
      .attr('stroke', d => getLinkColor(d.type as string))
      .attr('stroke-dasharray', d => d.type === 'missing-signature' ? '3,3' : null); // Dashed lines for missing signatures
    
    // Create node circles
    const nodeElements = svg.append('g')
      .selectAll('circle')
      .data(nodes)
      .join('circle')
      .attr('r', d => d.radius)
      .attr('fill', d => d.color)
      .attr('stroke', d => {
        if (d.isFederationReport && d.quorumValidation) {
          return d.quorumValidation.isSatisfied ? '#4CAF50' : '#F44336';
        }
        return '#fff';
      })
      .attr('stroke-width', d => d.isFederationReport ? 2.5 : 1.5)
      .style('cursor', 'pointer')
      .call(drag(simulation) as any);
    
    // Add text labels if enabled
    if (showLabels) {
      const labels = svg.append('g')
        .selectAll('text')
        .data(nodes)
        .join('text')
        .text(d => d.label)
        .attr('font-size', 10)
        .attr('dx', 12)
        .attr('dy', 4)
        .style('pointer-events', 'none');
    }
    
    // Add quorum indicators for federation reports
    svg.append('g')
      .selectAll('text')
      .data(nodes.filter(n => n.isFederationReport && n.quorumValidation))
      .join('text')
      .text(d => d.quorumValidation?.isSatisfied ? '✓' : '✗')
      .attr('font-size', 14)
      .attr('text-anchor', 'middle')
      .attr('dy', 5)
      .attr('fill', d => d.quorumValidation?.isSatisfied ? '#FFFFFF' : '#FFFFFF')
      .style('pointer-events', 'none');
    
    // Set up simulation
    const simulation = d3.forceSimulation(nodes)
      .force('link', d3.forceLink(links).id((d: any) => d.id).distance(d => {
        // Shorter distance for signer links
        if ((d as any).isSignerLink) return 50;
        return 100;
      }))
      .force('charge', d3.forceManyBody().strength(d => {
        // Less repulsion for signer nodes
        if ((d as Node).isSignerNode) return -100;
        return -400;
      }))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collide', d3.forceCollide().radius(d => (d as Node).radius * 1.5));
    
    // Group by thread if enabled
    if (groupByThread) {
      simulation.force('x', d3.forceX().x(d => {
        const node = d as Node;
        
        // Keep signers close to their federation report
        if (node.isSignerNode) {
          const reportLink = links.find(link => 
            typeof link.source === 'object' && 
            typeof link.target === 'object' && 
            link.isSignerLink && 
            (link.source.id === node.id || link.target.id === node.id)
          );
          
          if (reportLink) {
            const reportNode = reportLink.source === node ? reportLink.target : reportLink.source;
            if (typeof reportNode === 'object' && reportNode.x) {
              return reportNode.x;
            }
          }
        }
        
        if (!node.threadId) return width / 2;
        
        // Get hash code for thread ID to determine x position
        const hashCode = Array.from(node.threadId).reduce(
          (acc, char) => (acc << 5) - acc + char.charCodeAt(0), 0
        );
        return (Math.abs(hashCode) % 1000) / 1000 * width;
      }));
    }
    
    // Highlight selected node if enabled
    if (highlightSelected && selectedCredentialId) {
      nodeElements
        .attr('opacity', d => {
          // If selected node is a federation report, highlight its signers too
          if (d.id === selectedCredentialId) return 1;
          
          const selectedNode = nodes.find(n => n.id === selectedCredentialId);
          if (selectedNode?.isFederationReport && d.isSignerNode) {
            // Check if this signer is connected to the selected report
            const isConnected = links.some(link => 
              (typeof link.source === 'object' && link.source.id === d.id && 
               typeof link.target === 'object' && link.target.id === selectedCredentialId) ||
              (typeof link.target === 'object' && link.target.id === d.id && 
               typeof link.source === 'object' && link.source.id === selectedCredentialId)
            );
            return isConnected ? 1 : 0.3;
          }
          
          return 0.6;
        })
        .attr('stroke-width', d => d.id === selectedCredentialId ? 3 : (d.isFederationReport ? 2.5 : 1.5));
      
      linkElements
        .attr('opacity', d => {
          const source = typeof d.source === 'object' ? d.source.id : d.source;
          const target = typeof d.target === 'object' ? d.target.id : d.target;
          
          // Highlight links connected to selected node and its signers
          if (source === selectedCredentialId || target === selectedCredentialId) {
            return 1;
          }
          
          // If selected node is a federation report, highlight its signer links
          if (d.isSignerLink) {
            const report = typeof d.target === 'object' ? d.target : nodes.find(n => n.id === target);
            if (report && report.id === selectedCredentialId) {
              return 1;
            }
          }
          
          return 0.2;
        })
        .attr('stroke-width', d => {
          const source = typeof d.source === 'object' ? d.source.id : d.source;
          const target = typeof d.target === 'object' ? d.target.id : d.target;
          
          if (source === selectedCredentialId || target === selectedCredentialId) {
            return d.isSignerLink ? 2 + (d.signerWeight || 0) : 3;
          }
          
          if (d.isSignerLink) {
            const report = typeof d.target === 'object' ? d.target : nodes.find(n => n.id === target);
            if (report && report.id === selectedCredentialId) {
              return 2 + (d.signerWeight || 0);
            }
          }
          
          return d.isSignerLink ? 1 + (d.signerWeight || 0) : 1;
        });
    }
    
    // Handle node click
    nodeElements.on('click', (event, d) => {
      if (d.isSignerNode && onSignerSelect && d.signerDid && d.federationId) {
        onSignerSelect(d.signerDid, d.federationId);
      } else if (d.threadId && onThreadSelect) {
        onThreadSelect(d.threadId);
      } else if (onCredentialSelect) {
        onCredentialSelect(d.id);
      }
    });
    
    // Handle node hover
    nodeElements
      .on('mouseover', (event, d) => {
        setHoveredNode(d);
      })
      .on('mouseout', () => {
        setHoveredNode(null);
      });
    
    // Update positions on each simulation tick
    simulation.on('tick', () => {
      linkElements
        .attr('x1', d => (d.source as Node).x!)
        .attr('y1', d => (d.source as Node).y!)
        .attr('x2', d => (d.target as Node).x!)
        .attr('y2', d => (d.target as Node).y!);
      
      nodeElements
        .attr('cx', d => d.x!)
        .attr('cy', d => d.y!);
      
      if (showLabels) {
        svg.selectAll('text')
          .attr('x', d => (d as Node).x!)
          .attr('y', d => (d as Node).y!);
      }
    });
    
    // Clean up simulation when component unmounts
    return () => {
      simulation.stop();
    };
  }, [nodes, links, width, height, selectedCredentialId, showLabels, groupByThread, highlightSelected, onCredentialSelect, onSignerSelect, onThreadSelect]);
  
  // Drag handler for nodes
  function drag(simulation: d3.Simulation<Node, Link>) {
    function dragstarted(event: any) {
      if (!event.active) simulation.alphaTarget(0.3).restart();
      event.subject.fx = event.subject.x;
      event.subject.fy = event.subject.y;
    }
    
    function dragged(event: any) {
      event.subject.fx = event.x;
      event.subject.fy = event.y;
    }
    
    function dragended(event: any) {
      if (!event.active) simulation.alphaTarget(0);
      event.subject.fx = null;
      event.subject.fy = null;
    }
    
    return d3.drag()
      .on('start', dragstarted)
      .on('drag', dragged)
      .on('end', dragended);
  }
  
  // Helper function to get node color based on type
  function getNodeColor(type: string): string {
    const colors: Record<string, string> = {
      'proposal': '#4285F4',  // Blue
      'vote': '#34A853',      // Green
      'appeal': '#FBBC05',    // Yellow
      'appeal_vote': '#7CBB00', // Lime
      'finalization': '#00A1F1', // Light blue
      'appeal_finalization': '#F25022', // Light red
      'execution': '#8F00FF',  // Purple
      'FederationReport': '#673AB7', // Deep purple for federation reports
      'default': '#757575'    // Gray
    };
    
    if (Array.isArray(type)) {
      if (type.includes('FederationReport')) return colors['FederationReport'];
      return type.map(t => colors[t] || colors.default)[0] || colors.default;
    }
    
    return colors[type] || colors.default;
  }
  
  // Helper function to get link color based on type
  function getLinkColor(type: string): string {
    const colors: Record<string, string> = {
      'proposal_vote': '#4CAF50',
      'vote_finalization': '#2196F3',
      'finalization_execution': '#9C27B0',
      'proposal_appeal': '#FFC107',
      'appeal_appeal_vote': '#FF9800',
      'appeal_vote_appeal_finalization': '#F44336',
      'appeal_finalization_execution': '#E91E63',
      'signature': '#64B5F6', // Blue for signatures
      'missing-signature': '#BDBDBD', // Gray for missing signatures
      'default': '#999999'
    };
    
    return colors[type] || colors.default;
  }
  
  // Helper function to get link type based on connected node types
  function getLinkType(sourceType: string, targetType: string): string {
    // Handle array types
    if (Array.isArray(sourceType)) {
      sourceType = sourceType[0];
    }
    if (Array.isArray(targetType)) {
      targetType = targetType[0];
    }
    
    return `${sourceType}_${targetType}`;
  }
  
  // Helper function to get node radius based on type
  function getNodeRadius(type: string): number {
    const sizes: Record<string, number> = {
      'proposal': 12,
      'vote': 8,
      'appeal': 10,
      'appeal_vote': 8,
      'finalization': 10,
      'appeal_finalization': 10,
      'execution': 12,
      'FederationReport': 15, // Larger for federation reports
      'default': 8
    };
    
    if (Array.isArray(type)) {
      if (type.includes('FederationReport')) return sizes['FederationReport'];
      return type.map(t => sizes[t] || sizes.default)[0] || sizes.default;
    }
    
    return sizes[type] || sizes.default;
  }
  
  // Helper function to get node label based on credential
  function getNodeLabel(credential: WalletCredential): string {
    // For federation reports, include federation name
    if ((Array.isArray(credential.type) && credential.type.includes('FederationReport')) ||
        (credential.metadata && credential.metadata.federationMetadata)) {
      const fedName = credential.metadata?.federation?.name || 
                     credential.metadata?.federationMetadata?.name || 
                     'Federation';
      return `${fedName} Report`;
    }
    
    // Short label based on type and part of ID
    const shortId = credential.id.substring(credential.id.length - 6);
    return `${Array.isArray(credential.type) ? credential.type[0] : credential.type} (${shortId})`;
  }
  
  // Render tooltip content for different node types
  function renderTooltipContent(node: Node, credential?: WalletCredential): JSX.Element {
    // For signer nodes
    if (node.isSignerNode) {
      if (node.type === 'missing-signer') {
        return (
          <div>
            <h4>Missing Signer</h4>
            <p>{node.label}</p>
            <p><strong>Weight:</strong> {node.radius / 2}</p>
          </div>
        );
      }
      
      return (
        <div>
          <h4>Signer: {node.label}</h4>
          <p>DID: {node.signerDid}</p>
          {node.federationId && (
            <p>Federation: {federationManifests[node.federationId]?.name || node.federationId}</p>
          )}
          <p>Click to view signer details</p>
        </div>
      );
    }
    
    // For federation reports with quorum validation
    if (node.isFederationReport && node.quorumValidation) {
      return (
        <div>
          <h4>{node.label}</h4>
          <p><strong>Policy:</strong> {node.quorumValidation.policy}</p>
          <p><strong>Status:</strong> {node.quorumValidation.isSatisfied ? 
            '✅ Quorum Satisfied' : 
            '❌ Quorum Not Met'}</p>
          <p><strong>Signatures:</strong> {node.quorumValidation.actualApprovals}/{node.quorumValidation.requiredApprovals} required</p>
          
          {node.quorumValidation.requiredThreshold && (
            <p><strong>Threshold:</strong> {node.quorumValidation.actualThreshold}% / {node.quorumValidation.requiredThreshold}% required</p>
          )}
          
          {node.quorumValidation.totalWeight && (
            <p><strong>Total Weight:</strong> {node.quorumValidation.totalWeight}</p>
          )}
          
          <div style={{ marginTop: '8px' }}>
            <strong>Signers:</strong>
            <ul style={{ margin: '4px 0', paddingLeft: '16px' }}>
              {node.quorumValidation.signers.map((signer, i) => (
                <li key={i}>{signer.role} ({signer.did.split(':').pop()}) - weight: {signer.weight}</li>
              ))}
            </ul>
          </div>
        </div>
      );
    }
    
    // For regular credentials
    if (credential) {
      return (
        <div>
          <h4>{node.label}</h4>
          <p><strong>Type:</strong> {Array.isArray(credential.type) ? credential.type.join(', ') : credential.type}</p>
          <p><strong>ID:</strong> {credential.id.substring(0, 20)}...</p>
          <p><strong>Issued:</strong> {new Date(credential.issuanceDate).toLocaleDateString()}</p>
          {credential.credentialSubject.proposalId && (
            <p><strong>Proposal:</strong> {credential.credentialSubject.proposalId.substring(0, 10)}...</p>
          )}
          {credential.metadata?.agoranet?.threadId && (
            <p><strong>Thread:</strong> {credential.metadata.agoranet.threadId.substring(0, 10)}...</p>
          )}
        </div>
      );
    }
    
    return <div>No information available</div>;
  }
  
  // Render tooltip when hovering over a node
  const renderTooltip = () => {
    if (!hoveredNode) return null;
    
    // Find the credential for this node (if it's not a signer node)
    const credential = !hoveredNode.isSignerNode ? 
      credentials.find(c => c.id === hoveredNode.id) : 
      undefined;
    
    return (
      <div 
        ref={tooltipRef}
        style={{
          ...tooltipStyles,
          position: 'absolute',
          left: (hoveredNode.x || 0) + 20,
          top: (hoveredNode.y || 0) - 10,
          maxWidth: '300px',
        }}
      >
        {renderTooltipContent(hoveredNode, credential)}
      </div>
    );
  };
  
  return (
    <div style={{ position: 'relative', width, height }}>
      <svg ref={svgRef} />
      {renderTooltip()}
    </div>
  );
};

export default CredentialDAGView; 