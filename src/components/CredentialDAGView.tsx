import React, { useEffect, useRef, useState, useMemo } from 'react';
import * as d3 from 'd3';
import { WalletCredential } from '../../packages/credential-utils/types';
import { extractCredentialLineage } from '../../packages/credential-utils/utils/federationSignature';
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
}

// Link interface for D3 force graph
interface Link extends d3.SimulationLinkDatum<Node> {
  source: string | Node;
  target: string | Node;
  type: string;
}

// Props for the CredentialDAGView component
interface CredentialDAGViewProps {
  credentials: WalletCredential[];
  selectedCredentialId?: string;
  onCredentialSelect?: (id: string) => void;
  onThreadSelect?: (threadId: string) => void;
  width?: number;
  height?: number;
  showLabels?: boolean;
  groupByThread?: boolean;
  highlightSelected?: boolean;
}

/**
 * Component for visualizing credential lineage as a directed graph
 */
export const CredentialDAGView: React.FC<CredentialDAGViewProps> = ({
  credentials,
  selectedCredentialId,
  onCredentialSelect,
  onThreadSelect,
  width = 800,
  height = 600,
  showLabels = true,
  groupByThread = false,
  highlightSelected = true,
}) => {
  const svgRef = useRef<SVGSVGElement>(null);
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [links, setLinks] = useState<Link[]>([]);
  const [hoveredNode, setHoveredNode] = useState<Node | null>(null);

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
    const newNodes: Node[] = credentials.map(cred => {
      // Determine node type and properties
      const nodeType = cred.type;
      const proposalId = cred.credentialSubject.proposalId;
      const threadId = cred.metadata?.agoranet?.threadId;
      const federationId = cred.metadata?.agoranet?.federationId;
      
      // Get a label for the node
      const label = getNodeLabel(cred);
      
      // Get color based on credential type
      const color = getNodeColor(cred.type);
      
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
        color
      };
    });
    
    // Create links from lineage relationships
    const newLinks: Link[] = [];
    
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
    
    setNodes(newNodes);
    setLinks(newLinks);
  }, [credentials]);

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
      .attr('stroke-width', d => 2)
      .attr('stroke', d => getLinkColor(d.type as string));
    
    // Create node circles
    const nodeElements = svg.append('g')
      .selectAll('circle')
      .data(nodes)
      .join('circle')
      .attr('r', d => d.radius)
      .attr('fill', d => d.color)
      .attr('stroke', '#fff')
      .attr('stroke-width', 1.5)
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
    
    // Set up simulation
    const simulation = d3.forceSimulation(nodes)
      .force('link', d3.forceLink(links).id((d: any) => d.id).distance(100))
      .force('charge', d3.forceManyBody().strength(-400))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collide', d3.forceCollide().radius(d => (d as Node).radius * 1.5));
    
    // Group by thread if enabled
    if (groupByThread) {
      simulation.force('x', d3.forceX().x(d => {
        const node = d as Node;
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
        .attr('opacity', d => d.id === selectedCredentialId ? 1 : 0.6)
        .attr('stroke-width', d => d.id === selectedCredentialId ? 3 : 1.5);
      
      linkElements
        .attr('opacity', d => {
          const source = typeof d.source === 'object' ? d.source.id : d.source;
          const target = typeof d.target === 'object' ? d.target.id : d.target;
          return source === selectedCredentialId || target === selectedCredentialId ? 1 : 0.2;
        })
        .attr('stroke-width', d => {
          const source = typeof d.source === 'object' ? d.source.id : d.source;
          const target = typeof d.target === 'object' ? d.target.id : d.target;
          return source === selectedCredentialId || target === selectedCredentialId ? 3 : 1;
        });
    }
    
    // Handle node click
    nodeElements.on('click', (event, d) => {
      if (onCredentialSelect) {
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
  }, [nodes, links, width, height, selectedCredentialId, showLabels, groupByThread, highlightSelected, onCredentialSelect]);
  
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
      'default': '#757575'    // Gray
    };
    
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
      'default': '#999999'
    };
    
    return colors[type] || colors.default;
  }
  
  // Helper function to get link type based on connected node types
  function getLinkType(sourceType: string, targetType: string): string {
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
      'default': 8
    };
    
    return sizes[type] || sizes.default;
  }
  
  // Helper function to get node label based on credential
  function getNodeLabel(credential: WalletCredential): string {
    // Short label based on type and part of ID
    const shortId = credential.id.substring(credential.id.length - 6);
    return `${credential.type.charAt(0).toUpperCase() + credential.type.slice(1)} (${shortId})`;
  }
  
  // Get credential details for hover tooltip
  function getCredentialDetails(credential: WalletCredential): JSX.Element {
    return (
      <div>
        <p><strong>Type:</strong> {credential.type}</p>
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
  
  // Render tooltip when hovering over a node
  const renderTooltip = () => {
    if (!hoveredNode) return null;
    
    const credential = credentials.find(c => c.id === hoveredNode.id);
    if (!credential) return null;
    
    return (
      <div 
        ref={tooltipRef}
        style={{
          ...tooltipStyles,
          position: 'absolute',
          left: (hoveredNode.x || 0) + 20,
          top: (hoveredNode.y || 0) - 10,
        }}
      >
        <h4>{hoveredNode.label}</h4>
        {getCredentialDetails(credential)}
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