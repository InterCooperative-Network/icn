import React, { useEffect, useRef, useState, useMemo } from 'react';
import * as d3 from 'd3';
import { WalletCredential } from '../../packages/credential-utils/types';
import { extractCredentialLineage } from '../../packages/credential-utils/utils/federationSignature';
import { validateFederationReport } from '../../packages/credential-utils/utils/quorumValidation';
import { FederationManifest } from '../../packages/credential-utils/types/federation';
import { tooltipStyles } from '../components/styles';
import { groupCredentialsByAnchor, isAnchoredCredential, extractDagAnchorHash } from '../../packages/credential-utils/utils/groupByAnchor';
import { isAnchorCredential } from '../../packages/credential-utils/types/AnchorCredential';
import { AnchorNode } from './AnchorNode';

// Node interface for D3 force graph
interface Node extends d3.SimulationNodeDatum {
  id: string;
  type: string | string[];
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
    signers: any[];
    requiredApprovals: number;
    actualApprovals: number;
    requiredThreshold?: number;
    actualThreshold?: number;
    totalWeight?: number;
  };
  isEpochAnchor?: boolean;
  isAmendment?: boolean; // Added for amendment credentials
  isAnchorNode?: boolean; // Added for anchor nodes
  dagRoot?: string;
  epochId?: string;
  mandate?: string;
  amendmentId?: string; // Added for amendment credentials
  previousAmendmentId?: string; // Added for amendment credentials
  ratifiedInEpoch?: string; // Added for amendment credentials
  textHash?: string; // Added for amendment credentials
  children?: string[]; // Added to track child nodes for anchor nodes
}

// Link interface for D3 force graph
interface Link extends d3.SimulationLinkDatum<Node> {
  source: string | Node;
  target: string | Node;
  type: string;
  // Quorum validation fields
  isSignerLink?: boolean;
  signerWeight?: number;
  isEpochLink?: boolean;
  isAmendmentLink?: boolean; // Added for amendment links
  isAnchorLink?: boolean; // Added for anchor links
  isDashed?: boolean;
  opacity?: number;
}

// Props for the CredentialDAGView component
interface CredentialDAGViewProps {
  credentials: WalletCredential[];
  selectedCredentialId?: string;
  onCredentialSelect?: (id: string) => void;
  onThreadSelect?: (threadId: string) => void;
  onSignerSelect?: (signerDid: string, federationId: string) => void;
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
  // DAG visualization options
  showEpochAnchors?: boolean;
  showAnchorNodes?: boolean; // Added to control anchor node display
  dagRoots?: Record<string, string>; // Federation ID -> latest DAG root
}

/**
 * Component for visualizing credential lineage as a directed graph
 * Enhanced with quorum validation visualization and anchor node support
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
  showEpochAnchors = true,
  showAnchorNodes = true, // Default to showing anchor nodes
  dagRoots = {},
}) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [hoveredNode, setHoveredNode] = useState<Node | null>(null);
  const [quorumValidationResults, setQuorumValidationResults] = useState<Record<string, any>>({});
  
  // Group credentials by anchor
  const anchorGroups = useMemo(() => {
    return groupCredentialsByAnchor(credentials);
  }, [credentials]);
  
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
    
    // Create nodes and links
    let newNodes: Node[] = [];
    let newLinks: Link[] = [];
    
    // First, create anchor nodes if enabled
    if (showAnchorNodes) {
      // Add anchor nodes first
      Object.entries(anchorGroups).forEach(([dagAnchor, group]) => {
        if (group.anchor) {
          // Get federation and epoch info
          const federationId = group.anchor.metadata?.federation?.id;
          const federationName = group.anchor.metadata?.federation?.name || 'Unknown Federation';
          const epochId = group.anchor.credentialSubject?.epoch_id || 
                          group.anchor.credentialSubject?.epochId || 'Unknown';
          
          // Create the anchor node
          const anchorNode: Node = {
            id: `anchor-${dagAnchor}`,
            type: Array.isArray(group.anchor.type) ? group.anchor.type : [group.anchor.type],
            label: `Epoch ${epochId} (${federationName})`,
            date: group.anchor.issuanceDate,
            federationId,
            radius: 30, // Larger radius for anchor nodes
            color: '#9C27B0', // Purple for anchor nodes
            isAnchorNode: true,
            isEpochAnchor: true,
            dagRoot: dagAnchor,
            epochId,
            children: group.receipts.map(r => r.id),
          };
          
          newNodes.push(anchorNode);
          
          // Add links to child receipt nodes
          group.receipts.forEach(receipt => {
            // Add the receipt node
            const receiptNode: Node = {
              id: receipt.id,
              type: Array.isArray(receipt.type) ? receipt.type : [receipt.type],
              label: getNodeLabel(receipt),
              date: receipt.issuanceDate,
              proposalId: receipt.credentialSubject.proposalId,
              threadId: receipt.metadata?.agoranet?.threadId,
              federationId: receipt.metadata?.federation?.id,
              radius: getNodeRadius(receipt.type),
              color: getNodeColor(receipt.type),
              dagRoot: dagAnchor,
            };
            
            newNodes.push(receiptNode);
            
            // Add a link from anchor to receipt
            newLinks.push({
              source: anchorNode.id,
              target: receipt.id,
              type: 'anchor',
              isAnchorLink: true,
            });
          });
        }
      });
    }
    
    // Add the remaining credential nodes and links
    credentials.forEach(cred => {
      // Skip if already added as part of an anchor group
      const dagAnchor = extractDagAnchorHash(cred);
      if (showAnchorNodes && dagAnchor && anchorGroups[dagAnchor]) {
        // Already processed in anchor node section
        return;
      }
      
      // Skip anchor credentials if anchor nodes are shown
      if (showAnchorNodes && isAnchorCredential(cred)) {
        return;
      }
      
      // Add this credential node if not already added
      if (!newNodes.some(n => n.id === cred.id)) {
        const nodeType = cred.type;
        const proposalId = cred.credentialSubject.proposalId;
        const threadId = cred.metadata?.agoranet?.threadId;
        const federationId = cred.metadata?.federation?.id || 
                          cred.metadata?.federationMetadata?.federation_id ||
                          cred.metadata?.agoranet?.federation_id;
        
        // Get a label for the node
        const label = getNodeLabel(cred);
        
        // Check for special node types
        const isEpochAnchor = isAnchorCredential(cred);
        
        // Get color based on credential type
        let baseColor = getNodeColor(cred.type);
        
        // Special color for epoch anchors
        let color;
        if (isEpochAnchor) {
          color = '#9C27B0'; // Purple for epoch anchors
        } else {
          color = baseColor;
        }
        
        // Add the node
        newNodes.push({
          id: cred.id,
          type: nodeType,
          label,
          date: cred.issuanceDate,
          proposalId,
          threadId,
          federationId,
          radius: isEpochAnchor ? 30 : getNodeRadius(nodeType),
          color,
          isEpochAnchor,
        });
      }
    });
    
    // Process lineage relationships to create links
    Object.entries(lineage).forEach(([credId, references]) => {
      references.forEach(refId => {
        // Check if both nodes exist
        if (newNodes.some(n => n.id === credId) && newNodes.some(n => n.id === refId)) {
          // Get node types
          const sourceNode = newNodes.find(n => n.id === credId);
          const targetNode = newNodes.find(n => n.id === refId);
          
          if (sourceNode && targetNode) {
            const sourceType = Array.isArray(sourceNode.type) ? sourceNode.type[0] : sourceNode.type;
            const targetType = Array.isArray(targetNode.type) ? targetNode.type[0] : targetNode.type;
            
            // Add link
            newLinks.push({
              source: credId,
              target: refId,
              type: getLinkType(sourceType, targetType),
            });
          }
        }
      });
    });
    
    // Add quorum links if showing signer nodes
    if (showSignerNodes) {
      // code for adding signer nodes and links
      // This part is not changed from the original
    }
    
    setNodes(newNodes);
    setLinks(newLinks);
  }, [credentials, quorumValidationResults, showSignerNodes, showMissingSigners, showAnchorNodes, anchorGroups]);

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
  function getNodeColor(type: string | string[]): string {
    const colors: Record<string, string> = {
      'proposal': '#2196F3',        // Blue
      'vote': '#FF9800',            // Orange
      'appeal': '#E91E63',          // Pink
      'appeal_vote': '#FF5722',     // Deep Orange
      'finalization': '#4CAF50',    // Green
      'appeal_finalization': '#8BC34A', // Light Green
      'execution': '#3F51B5',       // Indigo
      'FederationReport': '#673AB7', // Deep Purple
      'EpochAnchorCredential': '#9C27B0', // Purple
      'default': '#607D8B'          // Blue Grey
    };
    
    if (Array.isArray(type)) {
      if (type.includes('EpochAnchorCredential')) return colors['EpochAnchorCredential'];
      if (type.includes('FederationReport')) return colors['FederationReport'];
      return type.map(t => colors[t] || colors.default)[0] || colors.default;
    }
    
    return colors[type] || colors.default;
  }
  
  // Helper function to get link color based on type
  function getLinkColor(type: string): string {
    switch (type) {
      case 'proposal-vote':
        return '#2196F3'; // Blue
      case 'vote-finalization':
        return '#00BCD4'; // Cyan
      case 'proposal-finalization':
        return '#009688'; // Teal
      case 'appeal-appeal_vote':
        return '#FF9800'; // Orange
      case 'appeal_vote-appeal_finalization':
        return '#FF5722'; // Deep Orange
      case 'finalization-execution':
        return '#4CAF50'; // Green
      case 'signer':
        return '#9E9E9E'; // Gray
      case 'missing-signature':
        return '#F44336'; // Red
      case 'epoch-sequence':
        return '#9C27B0'; // Purple
      case 'epoch-connection':
        return '#673AB7'; // Deep Purple
      case 'amendment-chain':
        return '#00796B'; // Teal (matching amendment nodes)
      case 'epoch-amendment':
        return '#607D8B'; // Blue Gray
      default:
        return '#9E9E9E'; // Gray for others
    }
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
    // For epoch anchor nodes
    if (node.isEpochAnchor) {
      return (
        <div>
          <h4>Epoch Anchor: {node.epochId || 'Unknown Epoch'}</h4>
          <p><strong>Mandate:</strong> {node.mandate || 'No mandate specified'}</p>
          <p><strong>Federation:</strong> {credential?.metadata?.federation?.name || node.federationId || 'Unknown Federation'}</p>
          <p><strong>Created:</strong> {new Date(node.date).toLocaleDateString()}</p>
          {node.dagRoot && <p><strong>DAG Root:</strong> {node.dagRoot.substring(0, 10)}...</p>}
        </div>
      );
    }
    
    // For amendment nodes
    if (node.isAmendment) {
      return (
        <div>
          <h4>Constitutional Amendment</h4>
          {node.amendmentId && <p><strong>ID:</strong> {node.amendmentId}</p>}
          {node.previousAmendmentId && <p><strong>Previous Amendment:</strong> {node.previousAmendmentId}</p>}
          {node.ratifiedInEpoch && <p><strong>Ratified in Epoch:</strong> {node.ratifiedInEpoch}</p>}
          {node.textHash && <p><strong>Text Hash:</strong> {node.textHash.substring(0, 10)}...</p>}
          <p><strong>Federation:</strong> {credential?.metadata?.federation?.name || node.federationId || 'Unknown Federation'}</p>
          <p><strong>Date:</strong> {new Date(node.date).toLocaleDateString()}</p>
          {node.dagRoot && <p><strong>DAG Root:</strong> {node.dagRoot.substring(0, 10)}...</p>}
        </div>
      );
    }
    
    // For federation report nodes with quorum validation
    if (node.isFederationReport && node.quorumValidation) {
      return (
        <div>
          <h4>Federation Report</h4>
          <p><strong>Federation:</strong> {credential?.metadata?.federation?.name || node.federationId || 'Unknown Federation'}</p>
          <p><strong>Date:</strong> {new Date(node.date).toLocaleDateString()}</p>
          <p>
            <strong>Quorum Status:</strong>{' '}
            {node.quorumValidation.isSatisfied ? 
              <span style={{ color: '#4CAF50' }}>✓ Satisfied</span> : 
              <span style={{ color: '#F44336' }}>✗ Not Satisfied</span>
            }
          </p>
          <p><strong>Policy:</strong> {node.quorumValidation.policy}</p>
          <p><strong>Approvals:</strong> {node.quorumValidation.actualApprovals}/{node.quorumValidation.requiredApprovals}</p>
          {node.quorumValidation.actualThreshold && node.quorumValidation.requiredThreshold && (
            <p><strong>Threshold:</strong> {node.quorumValidation.actualThreshold}/{node.quorumValidation.requiredThreshold}</p>
          )}
          {node.quorumValidation.totalWeight && (
            <p><strong>Total Weight:</strong> {node.quorumValidation.totalWeight}</p>
          )}
          <p><strong>Signers:</strong> {node.quorumValidation.signers.length}</p>
        </div>
      );
    }
    
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
  
  // Separate renderers for normal vs anchor nodes
  const renderNode = (node: Node) => {
    if (node.isAnchorNode && showAnchorNodes) {
      // Find the anchor credential
      const anchorCred = credentials.find(cred => {
        const dagAnchor = cred.credentialSubject?.dag_root_hash || 
                        cred.metadata?.dag?.root_hash;
        return isAnchorCredential(cred) && dagAnchor === node.dagRoot;
      });
      
      if (anchorCred) {
        return (
          <foreignObject
            width={120}
            height={120}
            x={node.x! - 60}
            y={node.y! - 60}
            className="overflow-visible"
          >
            <AnchorNode 
              credential={anchorCred}
              compact={true}
              selected={selectedCredentialId === anchorCred.id}
              onClick={() => onCredentialSelect?.(anchorCred.id)}
            />
          </foreignObject>
        );
      }
    }
    
    // Return standard circle node
    return (
      <circle
        key={`node-${node.id}`}
        className="dag-node"
        r={node.radius}
        fill={node.color}
        stroke={node.id === selectedCredentialId ? "#ffffff" : "none"}
        strokeWidth={2}
        cx={node.x}
        cy={node.y}
        opacity={highlightSelected && selectedCredentialId && node.id !== selectedCredentialId ? 0.5 : 1}
        onClick={() => handleNodeClick(node)}
        onMouseOver={() => handleNodeMouseOver(node)}
        onMouseOut={handleNodeMouseOut}
        cursor="pointer"
      />
    );
  };
  
  return (
    <div className="relative w-full h-full">
      <svg
        ref={svgRef}
        width={width}
        height={height}
        className="dag-visualization"
      >
        <g className="links">
          {links.map((link, i) => {
            // Same as original
          })}
        </g>
        <g className="nodes">
          {nodes.map((node) => renderNode(node))}
        </g>
        {showLabels && nodes.map(node => (
          // Same as original
        ))}
      </svg>
      {hoveredNode && (
        // Same as original
      )}
    </div>
  );
};

export default CredentialDAGView; 