using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.ServiceProcess;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

[DesignerGenerated]
internal class FormSettings : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("OnC")]
	private CheckBox _OnC;

	[CompilerGenerated]
	[AccessedThroughProperty("LogC")]
	private CheckBox _LogC;

	[CompilerGenerated]
	[AccessedThroughProperty("PrC")]
	private CheckBox _PrC;

	[CompilerGenerated]
	[AccessedThroughProperty("PrXc")]
	private CheckBox _PrXc;

	[CompilerGenerated]
	[AccessedThroughProperty("PrAc")]
	private CheckBox _PrAc;

	[CompilerGenerated]
	[AccessedThroughProperty("OffAc")]
	private CheckBox _OffAc;

	[CompilerGenerated]
	[AccessedThroughProperty("OffC")]
	private CheckBox _OffC;

	[CompilerGenerated]
	[AccessedThroughProperty("XmlC")]
	private CheckBox _XmlC;

	[CompilerGenerated]
	[AccessedThroughProperty("TxtC")]
	private CheckBox _TxtC;

	[CompilerGenerated]
	[AccessedThroughProperty("PdfC")]
	private CheckBox _PdfC;

	[CompilerGenerated]
	[AccessedThroughProperty("DlT")]
	private TextBox _DlT;

	[CompilerGenerated]
	[AccessedThroughProperty("MailB")]
	private Button _MailB;

	[CompilerGenerated]
	[AccessedThroughProperty("IndOt")]
	private TextBox _IndOt;

	[CompilerGenerated]
	[AccessedThroughProperty("IndYt")]
	private TextBox _IndYt;

	[CompilerGenerated]
	[AccessedThroughProperty("SelSwrver")]
	private Button _SelSwrver;

	[CompilerGenerated]
	[AccessedThroughProperty("AcsC")]
	private CheckBox _AcsC;

	[CompilerGenerated]
	[AccessedThroughProperty("TesB")]
	private Button _TesB;

	[CompilerGenerated]
	[AccessedThroughProperty("FisB")]
	private Button _FisB;

	[CompilerGenerated]
	[AccessedThroughProperty("MaxT")]
	private TextBox _MaxT;

	[CompilerGenerated]
	[AccessedThroughProperty("MinT")]
	private TextBox _MinT;

	[CompilerGenerated]
	[AccessedThroughProperty("VisC")]
	private CheckBox _VisC;

	[CompilerGenerated]
	[AccessedThroughProperty("Rb80")]
	private RadioButton _Rb80;

	[CompilerGenerated]
	[AccessedThroughProperty("Rb57")]
	private RadioButton _Rb57;

	[CompilerGenerated]
	[AccessedThroughProperty("MulC")]
	private CheckBox _MulC;

	[CompilerGenerated]
	[AccessedThroughProperty("BackupB")]
	private Button _BackupB;

	[CompilerGenerated]
	[AccessedThroughProperty("CBgov")]
	private CheckBox _CBgov;

	private bool UpLoadOrder;

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("FnT")]
	internal virtual TextBox FnT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label2")]
	internal virtual Label Label2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox OnC
	{
		[CompilerGenerated]
		get
		{
			return _OnC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OnC_CheckedChanged;
			CheckBox onC = _OnC;
			if (onC != null)
			{
				onC.CheckedChanged -= eventHandler;
			}
			_OnC = value;
			onC = _OnC;
			if (onC != null)
			{
				onC.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TinT")]
	internal virtual TextBox TinT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox LogC
	{
		[CompilerGenerated]
		get
		{
			return _LogC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = LogC_CheckedChanged;
			CheckBox logC = _LogC;
			if (logC != null)
			{
				logC.CheckedChanged -= eventHandler;
			}
			_LogC = value;
			logC = _LogC;
			if (logC != null)
			{
				logC.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox2")]
	internal virtual GroupBox GroupBox2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox PrC
	{
		[CompilerGenerated]
		get
		{
			return _PrC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PrC_CheckedChanged;
			CheckBox prC = _PrC;
			if (prC != null)
			{
				prC.CheckedChanged -= eventHandler;
			}
			_PrC = value;
			prC = _PrC;
			if (prC != null)
			{
				prC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox PrXc
	{
		[CompilerGenerated]
		get
		{
			return _PrXc;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PrXc_CheckedChanged;
			CheckBox prXc = _PrXc;
			if (prXc != null)
			{
				prXc.CheckedChanged -= eventHandler;
			}
			_PrXc = value;
			prXc = _PrXc;
			if (prXc != null)
			{
				prXc.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox PrAc
	{
		[CompilerGenerated]
		get
		{
			return _PrAc;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PrAc_CheckedChanged;
			CheckBox prAc = _PrAc;
			if (prAc != null)
			{
				prAc.CheckedChanged -= eventHandler;
			}
			_PrAc = value;
			prAc = _PrAc;
			if (prAc != null)
			{
				prAc.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox3")]
	internal virtual GroupBox GroupBox3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox OffAc
	{
		[CompilerGenerated]
		get
		{
			return _OffAc;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OffAc_CheckedChanged;
			CheckBox offAc = _OffAc;
			if (offAc != null)
			{
				offAc.CheckedChanged -= eventHandler;
			}
			_OffAc = value;
			offAc = _OffAc;
			if (offAc != null)
			{
				offAc.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox OffC
	{
		[CompilerGenerated]
		get
		{
			return _OffC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OffC_CheckedChanged;
			CheckBox offC = _OffC;
			if (offC != null)
			{
				offC.CheckedChanged -= eventHandler;
			}
			_OffC = value;
			offC = _OffC;
			if (offC != null)
			{
				offC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox XmlC
	{
		[CompilerGenerated]
		get
		{
			return _XmlC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = XmlC_CheckedChanged;
			CheckBox xmlC = _XmlC;
			if (xmlC != null)
			{
				xmlC.CheckedChanged -= eventHandler;
			}
			_XmlC = value;
			xmlC = _XmlC;
			if (xmlC != null)
			{
				xmlC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox TxtC
	{
		[CompilerGenerated]
		get
		{
			return _TxtC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TxtC_CheckedChanged;
			CheckBox txtC = _TxtC;
			if (txtC != null)
			{
				txtC.CheckedChanged -= eventHandler;
			}
			_TxtC = value;
			txtC = _TxtC;
			if (txtC != null)
			{
				txtC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox PdfC
	{
		[CompilerGenerated]
		get
		{
			return _PdfC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = PdfC_CheckedChanged;
			CheckBox pdfC = _PdfC;
			if (pdfC != null)
			{
				pdfC.CheckedChanged -= eventHandler;
			}
			_PdfC = value;
			pdfC = _PdfC;
			if (pdfC != null)
			{
				pdfC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual TextBox DlT
	{
		[CompilerGenerated]
		get
		{
			return _DlT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = DlT_TextChanged;
			TextBox dlT = _DlT;
			if (dlT != null)
			{
				((Control)dlT).TextChanged -= eventHandler;
			}
			_DlT = value;
			dlT = _DlT;
			if (dlT != null)
			{
				((Control)dlT).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button MailB
	{
		[CompilerGenerated]
		get
		{
			return _MailB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = MailB_Click;
			Button mailB = _MailB;
			if (mailB != null)
			{
				((Control)mailB).Click -= eventHandler;
			}
			_MailB = value;
			mailB = _MailB;
			if (mailB != null)
			{
				((Control)mailB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox4")]
	internal virtual GroupBox GroupBox4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label5")]
	internal virtual Label Label5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox IndOt
	{
		[CompilerGenerated]
		get
		{
			return _IndOt;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = IndOt_TextChanged;
			TextBox indOt = _IndOt;
			if (indOt != null)
			{
				((Control)indOt).TextChanged -= eventHandler;
			}
			_IndOt = value;
			indOt = _IndOt;
			if (indOt != null)
			{
				((Control)indOt).TextChanged += eventHandler;
			}
		}
	}

	internal virtual TextBox IndYt
	{
		[CompilerGenerated]
		get
		{
			return _IndYt;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = IndYt_TextChanged;
			TextBox indYt = _IndYt;
			if (indYt != null)
			{
				((Control)indYt).TextChanged -= eventHandler;
			}
			_IndYt = value;
			indYt = _IndYt;
			if (indYt != null)
			{
				((Control)indYt).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Server")]
	internal virtual TextBox Server
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button SelSwrver
	{
		[CompilerGenerated]
		get
		{
			return _SelSwrver;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click -= eventHandler;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				((Control)selSwrver).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label21")]
	internal virtual Label Label21
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox AcsC
	{
		[CompilerGenerated]
		get
		{
			return _AcsC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = AcsC_CheckedChanged;
			CheckBox acsC = _AcsC;
			if (acsC != null)
			{
				acsC.CheckedChanged -= eventHandler;
			}
			_AcsC = value;
			acsC = _AcsC;
			if (acsC != null)
			{
				acsC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual Button TesB
	{
		[CompilerGenerated]
		get
		{
			return _TesB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = TesB_Click;
			Button tesB = _TesB;
			if (tesB != null)
			{
				((Control)tesB).Click -= eventHandler;
			}
			_TesB = value;
			tesB = _TesB;
			if (tesB != null)
			{
				((Control)tesB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("RejT")]
	internal virtual TextBox RejT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button FisB
	{
		[CompilerGenerated]
		get
		{
			return _FisB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = FisB_Click;
			Button fisB = _FisB;
			if (fisB != null)
			{
				((Control)fisB).Click -= eventHandler;
			}
			_FisB = value;
			fisB = _FisB;
			if (fisB != null)
			{
				((Control)fisB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label6")]
	internal virtual Label Label6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual TextBox MaxT
	{
		[CompilerGenerated]
		get
		{
			return _MaxT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = MaxT_TextChanged;
			TextBox maxT = _MaxT;
			if (maxT != null)
			{
				((Control)maxT).TextChanged -= eventHandler;
			}
			_MaxT = value;
			maxT = _MaxT;
			if (maxT != null)
			{
				((Control)maxT).TextChanged += eventHandler;
			}
		}
	}

	internal virtual TextBox MinT
	{
		[CompilerGenerated]
		get
		{
			return _MinT;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = MinT_TextChanged;
			TextBox minT = _MinT;
			if (minT != null)
			{
				((Control)minT).TextChanged -= eventHandler;
			}
			_MinT = value;
			minT = _MinT;
			if (minT != null)
			{
				((Control)minT).TextChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label8")]
	internal virtual Label Label8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label7")]
	internal virtual Label Label7
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox VisC
	{
		[CompilerGenerated]
		get
		{
			return _VisC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = VisC_CheckedChanged;
			CheckBox visC = _VisC;
			if (visC != null)
			{
				visC.CheckedChanged -= eventHandler;
			}
			_VisC = value;
			visC = _VisC;
			if (visC != null)
			{
				visC.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual RadioButton Rb80
	{
		[CompilerGenerated]
		get
		{
			return _Rb80;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Rb80_CheckedChanged;
			RadioButton rb = _Rb80;
			if (rb != null)
			{
				rb.CheckedChanged -= eventHandler;
			}
			_Rb80 = value;
			rb = _Rb80;
			if (rb != null)
			{
				rb.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual RadioButton Rb57
	{
		[CompilerGenerated]
		get
		{
			return _Rb57;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Rb57_CheckedChanged;
			RadioButton rb = _Rb57;
			if (rb != null)
			{
				rb.CheckedChanged -= eventHandler;
			}
			_Rb57 = value;
			rb = _Rb57;
			if (rb != null)
			{
				rb.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual CheckBox MulC
	{
		[CompilerGenerated]
		get
		{
			return _MulC;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = MulC_CheckedChanged;
			CheckBox mulC = _MulC;
			if (mulC != null)
			{
				mulC.CheckedChanged -= eventHandler;
			}
			_MulC = value;
			mulC = _MulC;
			if (mulC != null)
			{
				mulC.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox5")]
	internal virtual GroupBox GroupBox5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button BackupB
	{
		[CompilerGenerated]
		get
		{
			return _BackupB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = BackupB_Click;
			Button backupB = _BackupB;
			if (backupB != null)
			{
				((Control)backupB).Click -= eventHandler;
			}
			_BackupB = value;
			backupB = _BackupB;
			if (backupB != null)
			{
				((Control)backupB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label10")]
	internal virtual Label Label10
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label9")]
	internal virtual Label Label9
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("lText")]
	internal virtual TextBox lText
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("fText")]
	internal virtual TextBox fText
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("LabelService")]
	internal virtual Label LabelService
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox6")]
	internal virtual GroupBox GroupBox6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TBT")]
	internal virtual TextBox TBT
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("LabelService1")]
	internal virtual Label LabelService1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabControlAll")]
	internal virtual TabControl TabControlAll
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabPage1")]
	internal virtual TabPage TabPage1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TabPage2")]
	internal virtual TabPage TabPage2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GroupBox7")]
	internal virtual GroupBox GroupBox7
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox CBgov
	{
		[CompilerGenerated]
		get
		{
			return _CBgov;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CBgov_CheckedChanged;
			CheckBox cBgov = _CBgov;
			if (cBgov != null)
			{
				cBgov.CheckedChanged -= eventHandler;
			}
			_CBgov = value;
			cBgov = _CBgov;
			if (cBgov != null)
			{
				cBgov.CheckedChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox8")]
	internal virtual GroupBox GroupBox8
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormSettings()
	{
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		((Form)this).Load += FormSettings_Load;
		((Form)this).Closing += FormSettings_Closing;
		((Form)this).FormClosing += new FormClosingEventHandler(FormSettings_FormClosing);
		UpLoadOrder = false;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_0074: Unknown result type (might be due to invalid IL or missing references)
		//IL_007e: Expected O, but got Unknown
		//IL_007f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0089: Expected O, but got Unknown
		//IL_008a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0094: Expected O, but got Unknown
		//IL_0095: Unknown result type (might be due to invalid IL or missing references)
		//IL_009f: Expected O, but got Unknown
		//IL_00a0: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Expected O, but got Unknown
		//IL_00ab: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b5: Expected O, but got Unknown
		//IL_00b6: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c0: Expected O, but got Unknown
		//IL_00c1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cb: Expected O, but got Unknown
		//IL_00cc: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d6: Expected O, but got Unknown
		//IL_00d7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e1: Expected O, but got Unknown
		//IL_00e2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ec: Expected O, but got Unknown
		//IL_00ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f7: Expected O, but got Unknown
		//IL_00f8: Unknown result type (might be due to invalid IL or missing references)
		//IL_0102: Expected O, but got Unknown
		//IL_0103: Unknown result type (might be due to invalid IL or missing references)
		//IL_010d: Expected O, but got Unknown
		//IL_010e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0118: Expected O, but got Unknown
		//IL_0119: Unknown result type (might be due to invalid IL or missing references)
		//IL_0123: Expected O, but got Unknown
		//IL_0124: Unknown result type (might be due to invalid IL or missing references)
		//IL_012e: Expected O, but got Unknown
		//IL_012f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0139: Expected O, but got Unknown
		//IL_013a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0144: Expected O, but got Unknown
		//IL_0145: Unknown result type (might be due to invalid IL or missing references)
		//IL_014f: Expected O, but got Unknown
		//IL_0150: Unknown result type (might be due to invalid IL or missing references)
		//IL_015a: Expected O, but got Unknown
		//IL_015b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0165: Expected O, but got Unknown
		//IL_0166: Unknown result type (might be due to invalid IL or missing references)
		//IL_0170: Expected O, but got Unknown
		//IL_0171: Unknown result type (might be due to invalid IL or missing references)
		//IL_017b: Expected O, but got Unknown
		//IL_017c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0186: Expected O, but got Unknown
		//IL_0187: Unknown result type (might be due to invalid IL or missing references)
		//IL_0191: Expected O, but got Unknown
		//IL_0192: Unknown result type (might be due to invalid IL or missing references)
		//IL_019c: Expected O, but got Unknown
		//IL_019d: Unknown result type (might be due to invalid IL or missing references)
		//IL_01a7: Expected O, but got Unknown
		//IL_01a8: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b2: Expected O, but got Unknown
		//IL_01b3: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bd: Expected O, but got Unknown
		//IL_01be: Unknown result type (might be due to invalid IL or missing references)
		//IL_01c8: Expected O, but got Unknown
		//IL_01c9: Unknown result type (might be due to invalid IL or missing references)
		//IL_01d3: Expected O, but got Unknown
		//IL_01d4: Unknown result type (might be due to invalid IL or missing references)
		//IL_01de: Expected O, but got Unknown
		//IL_01df: Unknown result type (might be due to invalid IL or missing references)
		//IL_01e9: Expected O, but got Unknown
		//IL_01ea: Unknown result type (might be due to invalid IL or missing references)
		//IL_01f4: Expected O, but got Unknown
		//IL_01f5: Unknown result type (might be due to invalid IL or missing references)
		//IL_01ff: Expected O, but got Unknown
		//IL_0200: Unknown result type (might be due to invalid IL or missing references)
		//IL_020a: Expected O, but got Unknown
		//IL_020b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0215: Expected O, but got Unknown
		//IL_0216: Unknown result type (might be due to invalid IL or missing references)
		//IL_0220: Expected O, but got Unknown
		//IL_0221: Unknown result type (might be due to invalid IL or missing references)
		//IL_022b: Expected O, but got Unknown
		//IL_022c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0236: Expected O, but got Unknown
		//IL_0237: Unknown result type (might be due to invalid IL or missing references)
		//IL_0241: Expected O, but got Unknown
		//IL_0242: Unknown result type (might be due to invalid IL or missing references)
		//IL_024c: Expected O, but got Unknown
		//IL_024d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0257: Expected O, but got Unknown
		//IL_0258: Unknown result type (might be due to invalid IL or missing references)
		//IL_0262: Expected O, but got Unknown
		//IL_0263: Unknown result type (might be due to invalid IL or missing references)
		//IL_026d: Expected O, but got Unknown
		//IL_026e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0278: Expected O, but got Unknown
		//IL_0279: Unknown result type (might be due to invalid IL or missing references)
		//IL_0283: Expected O, but got Unknown
		//IL_03a3: Unknown result type (might be due to invalid IL or missing references)
		//IL_03ad: Expected O, but got Unknown
		//IL_042b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0435: Expected O, but got Unknown
		//IL_04b0: Unknown result type (might be due to invalid IL or missing references)
		//IL_04ba: Expected O, but got Unknown
		//IL_057c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0586: Expected O, but got Unknown
		//IL_0607: Unknown result type (might be due to invalid IL or missing references)
		//IL_0611: Expected O, but got Unknown
		//IL_0688: Unknown result type (might be due to invalid IL or missing references)
		//IL_0692: Expected O, but got Unknown
		//IL_07a7: Unknown result type (might be due to invalid IL or missing references)
		//IL_07b1: Expected O, but got Unknown
		//IL_083e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0848: Expected O, but got Unknown
		//IL_08c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_08d0: Expected O, but got Unknown
		//IL_0956: Unknown result type (might be due to invalid IL or missing references)
		//IL_0960: Expected O, but got Unknown
		//IL_09de: Unknown result type (might be due to invalid IL or missing references)
		//IL_09e8: Expected O, but got Unknown
		//IL_0a63: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a6d: Expected O, but got Unknown
		//IL_0ae8: Unknown result type (might be due to invalid IL or missing references)
		//IL_0af2: Expected O, but got Unknown
		//IL_0b6c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0b76: Expected O, but got Unknown
		//IL_0c4c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0c56: Expected O, but got Unknown
		//IL_0ce3: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ced: Expected O, but got Unknown
		//IL_0d74: Unknown result type (might be due to invalid IL or missing references)
		//IL_0d7e: Expected O, but got Unknown
		//IL_0de9: Unknown result type (might be due to invalid IL or missing references)
		//IL_0df3: Expected O, but got Unknown
		//IL_0e7a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0e84: Expected O, but got Unknown
		//IL_100d: Unknown result type (might be due to invalid IL or missing references)
		//IL_1017: Expected O, but got Unknown
		//IL_10a1: Unknown result type (might be due to invalid IL or missing references)
		//IL_10ab: Expected O, but got Unknown
		//IL_1132: Unknown result type (might be due to invalid IL or missing references)
		//IL_113c: Expected O, but got Unknown
		//IL_11dd: Unknown result type (might be due to invalid IL or missing references)
		//IL_11e7: Expected O, but got Unknown
		//IL_1265: Unknown result type (might be due to invalid IL or missing references)
		//IL_126f: Expected O, but got Unknown
		//IL_12ea: Unknown result type (might be due to invalid IL or missing references)
		//IL_12f4: Expected O, but got Unknown
		//IL_1366: Unknown result type (might be due to invalid IL or missing references)
		//IL_1370: Expected O, but got Unknown
		//IL_13de: Unknown result type (might be due to invalid IL or missing references)
		//IL_13e8: Expected O, but got Unknown
		//IL_1462: Unknown result type (might be due to invalid IL or missing references)
		//IL_146c: Expected O, but got Unknown
		//IL_14ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_14f7: Expected O, but got Unknown
		//IL_1569: Unknown result type (might be due to invalid IL or missing references)
		//IL_1573: Expected O, but got Unknown
		//IL_15de: Unknown result type (might be due to invalid IL or missing references)
		//IL_15e8: Expected O, but got Unknown
		//IL_165c: Unknown result type (might be due to invalid IL or missing references)
		//IL_1666: Expected O, but got Unknown
		//IL_16ed: Unknown result type (might be due to invalid IL or missing references)
		//IL_16f7: Expected O, but got Unknown
		//IL_1864: Unknown result type (might be due to invalid IL or missing references)
		//IL_186e: Expected O, but got Unknown
		//IL_19f9: Unknown result type (might be due to invalid IL or missing references)
		//IL_1a03: Expected O, but got Unknown
		//IL_1a75: Unknown result type (might be due to invalid IL or missing references)
		//IL_1a7f: Expected O, but got Unknown
		//IL_1afc: Unknown result type (might be due to invalid IL or missing references)
		//IL_1b06: Expected O, but got Unknown
		//IL_1b90: Unknown result type (might be due to invalid IL or missing references)
		//IL_1b9a: Expected O, but got Unknown
		//IL_1c30: Unknown result type (might be due to invalid IL or missing references)
		//IL_1c3a: Expected O, but got Unknown
		//IL_1cc4: Unknown result type (might be due to invalid IL or missing references)
		//IL_1cce: Expected O, but got Unknown
		//IL_1d58: Unknown result type (might be due to invalid IL or missing references)
		//IL_1d62: Expected O, but got Unknown
		//IL_1de9: Unknown result type (might be due to invalid IL or missing references)
		//IL_1df3: Expected O, but got Unknown
		//IL_1e7a: Unknown result type (might be due to invalid IL or missing references)
		//IL_1e84: Expected O, but got Unknown
		//IL_1f0b: Unknown result type (might be due to invalid IL or missing references)
		//IL_1f15: Expected O, but got Unknown
		//IL_1f8b: Unknown result type (might be due to invalid IL or missing references)
		//IL_1f95: Expected O, but got Unknown
		//IL_200c: Unknown result type (might be due to invalid IL or missing references)
		//IL_2016: Expected O, but got Unknown
		//IL_209d: Unknown result type (might be due to invalid IL or missing references)
		//IL_20a7: Expected O, but got Unknown
		//IL_2121: Unknown result type (might be due to invalid IL or missing references)
		//IL_212b: Expected O, but got Unknown
		//IL_21c6: Unknown result type (might be due to invalid IL or missing references)
		//IL_21d0: Expected O, but got Unknown
		//IL_2299: Unknown result type (might be due to invalid IL or missing references)
		//IL_22a3: Expected O, but got Unknown
		//IL_22cd: Unknown result type (might be due to invalid IL or missing references)
		//IL_2385: Unknown result type (might be due to invalid IL or missing references)
		//IL_23e8: Unknown result type (might be due to invalid IL or missing references)
		//IL_23f2: Expected O, but got Unknown
		//IL_247f: Unknown result type (might be due to invalid IL or missing references)
		//IL_2489: Expected O, but got Unknown
		//IL_251a: Unknown result type (might be due to invalid IL or missing references)
		//IL_2524: Expected O, but got Unknown
		//IL_2643: Unknown result type (might be due to invalid IL or missing references)
		//IL_264d: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormSettings));
		GroupBox1 = new GroupBox();
		FisB = new Button();
		TesB = new Button();
		RejT = new TextBox();
		GroupBox6 = new GroupBox();
		TBT = new TextBox();
		MailB = new Button();
		GroupBox5 = new GroupBox();
		LabelService1 = new Label();
		lText = new TextBox();
		LabelService = new Label();
		Label9 = new Label();
		Label10 = new Label();
		fText = new TextBox();
		BackupB = new Button();
		GroupBox4 = new GroupBox();
		AcsC = new CheckBox();
		Server = new TextBox();
		SelSwrver = new Button();
		Label21 = new Label();
		GroupBox3 = new GroupBox();
		VisC = new CheckBox();
		MulC = new CheckBox();
		Label8 = new Label();
		Label7 = new Label();
		Label6 = new Label();
		MaxT = new TextBox();
		MinT = new TextBox();
		Label5 = new Label();
		Label4 = new Label();
		IndOt = new TextBox();
		IndYt = new TextBox();
		OffAc = new CheckBox();
		OffC = new CheckBox();
		GroupBox2 = new GroupBox();
		Rb80 = new RadioButton();
		Rb57 = new RadioButton();
		Label3 = new Label();
		DlT = new TextBox();
		XmlC = new CheckBox();
		LogC = new CheckBox();
		TxtC = new CheckBox();
		PdfC = new CheckBox();
		PrAc = new CheckBox();
		PrXc = new CheckBox();
		PrC = new CheckBox();
		FnT = new TextBox();
		Label2 = new Label();
		OnC = new CheckBox();
		TinT = new TextBox();
		Label1 = new Label();
		TabControlAll = new TabControl();
		TabPage1 = new TabPage();
		TabPage2 = new TabPage();
		GroupBox7 = new GroupBox();
		CBgov = new CheckBox();
		GroupBox8 = new GroupBox();
		((Control)GroupBox1).SuspendLayout();
		((Control)GroupBox6).SuspendLayout();
		((Control)GroupBox5).SuspendLayout();
		((Control)GroupBox4).SuspendLayout();
		((Control)GroupBox3).SuspendLayout();
		((Control)GroupBox2).SuspendLayout();
		((Control)TabControlAll).SuspendLayout();
		((Control)TabPage1).SuspendLayout();
		((Control)TabPage2).SuspendLayout();
		((Control)GroupBox8).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)GroupBox1).Controls.Add((Control)(object)FisB);
		((Control)GroupBox1).Controls.Add((Control)(object)TesB);
		((Control)GroupBox1).Controls.Add((Control)(object)RejT);
		((Control)GroupBox1).Location = new Point(12, 52);
		((Control)GroupBox1).Name = "GroupBox1";
		((Control)GroupBox1).Size = new Size(951, 75);
		((Control)GroupBox1).TabIndex = 0;
		GroupBox1.TabStop = false;
		((Control)FisB).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FisB).Location = new Point(749, 27);
		((Control)FisB).Name = "FisB";
		((Control)FisB).Size = new Size(186, 35);
		((Control)FisB).TabIndex = 27;
		((ButtonBase)FisB).Text = "Фіскальний";
		((ButtonBase)FisB).UseVisualStyleBackColor = true;
		((Control)TesB).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TesB).Location = new Point(20, 27);
		((Control)TesB).Name = "TesB";
		((Control)TesB).Size = new Size(186, 35);
		((Control)TesB).TabIndex = 26;
		((ButtonBase)TesB).Text = "Тестовий";
		((ButtonBase)TesB).UseVisualStyleBackColor = true;
		((Control)RejT).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((TextBoxBase)RejT).ForeColor = Color.Black;
		((Control)RejT).Location = new Point(246, 26);
		((Control)RejT).Name = "RejT";
		((TextBoxBase)RejT).ReadOnly = true;
		((Control)RejT).Size = new Size(462, 34);
		((Control)RejT).TabIndex = 24;
		((Control)RejT).TabStop = false;
		RejT.TextAlign = (HorizontalAlignment)2;
		((Control)GroupBox6).Controls.Add((Control)(object)TBT);
		((Control)GroupBox6).Controls.Add((Control)(object)MailB);
		((Control)GroupBox6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox6).Location = new Point(533, 362);
		((Control)GroupBox6).Name = "GroupBox6";
		((Control)GroupBox6).Size = new Size(408, 95);
		((Control)GroupBox6).TabIndex = 29;
		GroupBox6.TabStop = false;
		GroupBox6.Text = "Налаштування закриття зміни";
		((Control)TBT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TBT).Location = new Point(25, 42);
		((Control)TBT).Name = "TBT";
		((TextBoxBase)TBT).ReadOnly = true;
		((Control)TBT).Size = new Size(263, 30);
		((Control)TBT).TabIndex = 26;
		TBT.TextAlign = (HorizontalAlignment)2;
		((Control)MailB).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)MailB).Location = new Point(307, 42);
		((Control)MailB).Name = "MailB";
		((Control)MailB).Size = new Size(86, 27);
		((Control)MailB).TabIndex = 23;
		((ButtonBase)MailB).Text = "...";
		((ButtonBase)MailB).UseVisualStyleBackColor = true;
		((Control)GroupBox5).Controls.Add((Control)(object)LabelService1);
		((Control)GroupBox5).Controls.Add((Control)(object)lText);
		((Control)GroupBox5).Controls.Add((Control)(object)LabelService);
		((Control)GroupBox5).Controls.Add((Control)(object)Label9);
		((Control)GroupBox5).Controls.Add((Control)(object)Label10);
		((Control)GroupBox5).Controls.Add((Control)(object)fText);
		((Control)GroupBox5).Controls.Add((Control)(object)BackupB);
		((Control)GroupBox5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox5).Location = new Point(26, 325);
		((Control)GroupBox5).Name = "GroupBox5";
		((Control)GroupBox5).Size = new Size(915, 136);
		((Control)GroupBox5).TabIndex = 28;
		GroupBox5.TabStop = false;
		GroupBox5.Text = "Backup";
		LabelService1.AutoSize = true;
		((Control)LabelService1).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)LabelService1).Location = new Point(503, 20);
		((Control)LabelService1).Name = "LabelService1";
		((Control)LabelService1).Size = new Size(340, 24);
		((Control)LabelService1).TabIndex = 33;
		LabelService1.Text = "Служба WebСheckPRROManagement";
		((Control)lText).Enabled = false;
		((Control)lText).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)lText).Location = new Point(168, 92);
		((Control)lText).Name = "lText";
		((TextBoxBase)lText).ReadOnly = true;
		((Control)lText).Size = new Size(317, 30);
		((Control)lText).TabIndex = 30;
		lText.TextAlign = (HorizontalAlignment)2;
		LabelService.AutoSize = true;
		((Control)LabelService).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)LabelService).Location = new Point(503, 49);
		((Control)LabelService).Name = "LabelService";
		((Control)LabelService).Size = new Size(292, 24);
		((Control)LabelService).TabIndex = 32;
		LabelService.Text = "Служба WebСheckPRROBackup";
		Label9.AutoSize = true;
		((Control)Label9).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label9).Location = new Point(10, 48);
		((Control)Label9).Name = "Label9";
		((Control)Label9).Size = new Size(138, 24);
		((Control)Label9).TabIndex = 31;
		Label9.Text = "Перший запис";
		Label10.AutoSize = true;
		((Control)Label10).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label10).Location = new Point(10, 97);
		((Control)Label10).Name = "Label10";
		((Control)Label10).Size = new Size(149, 24);
		((Control)Label10).TabIndex = 32;
		Label10.Text = "Останній запис";
		((Control)fText).Enabled = false;
		((Control)fText).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)fText).Location = new Point(165, 43);
		((Control)fText).Name = "fText";
		((TextBoxBase)fText).ReadOnly = true;
		((Control)fText).Size = new Size(317, 30);
		((Control)fText).TabIndex = 29;
		fText.TextAlign = (HorizontalAlignment)2;
		((Control)BackupB).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)BackupB).Location = new Point(534, 91);
		((Control)BackupB).Name = "BackupB";
		((Control)BackupB).Size = new Size(353, 35);
		((Control)BackupB).TabIndex = 28;
		((ButtonBase)BackupB).Text = "Увімкнути";
		((ButtonBase)BackupB).UseVisualStyleBackColor = true;
		((Control)GroupBox4).Controls.Add((Control)(object)AcsC);
		((Control)GroupBox4).Controls.Add((Control)(object)Server);
		((Control)GroupBox4).Controls.Add((Control)(object)SelSwrver);
		((Control)GroupBox4).Controls.Add((Control)(object)Label21);
		((Control)GroupBox4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox4).Location = new Point(24, 318);
		((Control)GroupBox4).Name = "GroupBox4";
		((Control)GroupBox4).Size = new Size(486, 142);
		((Control)GroupBox4).TabIndex = 23;
		GroupBox4.TabStop = false;
		GroupBox4.Text = "Підпис та відправка";
		((ButtonBase)AcsC).AutoSize = true;
		((Control)AcsC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)AcsC).Location = new Point(82, 97);
		((Control)AcsC).Name = "AcsC";
		((Control)AcsC).Size = new Size(297, 29);
		((Control)AcsC).TabIndex = 24;
		((ButtonBase)AcsC).Text = "Використовувати ACSKTSP";
		((ButtonBase)AcsC).UseVisualStyleBackColor = true;
		((Control)Server).Enabled = false;
		((Control)Server).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Server).Location = new Point(97, 47);
		((Control)Server).Name = "Server";
		((Control)Server).Size = new Size(253, 30);
		((Control)Server).TabIndex = 23;
		Server.TextAlign = (HorizontalAlignment)2;
		((Control)SelSwrver).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)SelSwrver).Location = new Point(375, 47);
		((Control)SelSwrver).Name = "SelSwrver";
		((Control)SelSwrver).Size = new Size(86, 30);
		((Control)SelSwrver).TabIndex = 22;
		((ButtonBase)SelSwrver).Text = "...";
		((ButtonBase)SelSwrver).UseVisualStyleBackColor = true;
		Label21.AutoSize = true;
		((Control)Label21).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label21).Location = new Point(6, 50);
		((Control)Label21).Name = "Label21";
		((Control)Label21).Size = new Size(64, 25);
		((Control)Label21).TabIndex = 21;
		Label21.Text = "АЦСК";
		((Control)GroupBox3).Controls.Add((Control)(object)VisC);
		((Control)GroupBox3).Controls.Add((Control)(object)MulC);
		((Control)GroupBox3).Controls.Add((Control)(object)Label8);
		((Control)GroupBox3).Controls.Add((Control)(object)Label7);
		((Control)GroupBox3).Controls.Add((Control)(object)Label6);
		((Control)GroupBox3).Controls.Add((Control)(object)MaxT);
		((Control)GroupBox3).Controls.Add((Control)(object)MinT);
		((Control)GroupBox3).Controls.Add((Control)(object)Label5);
		((Control)GroupBox3).Controls.Add((Control)(object)Label4);
		((Control)GroupBox3).Controls.Add((Control)(object)IndOt);
		((Control)GroupBox3).Controls.Add((Control)(object)IndYt);
		((Control)GroupBox3).Controls.Add((Control)(object)OffAc);
		((Control)GroupBox3).Controls.Add((Control)(object)OffC);
		((Control)GroupBox3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox3).Location = new Point(24, 20);
		((Control)GroupBox3).Name = "GroupBox3";
		((Control)GroupBox3).Size = new Size(486, 292);
		((Control)GroupBox3).TabIndex = 22;
		GroupBox3.TabStop = false;
		GroupBox3.Text = "Офлайн";
		((ButtonBase)VisC).AutoSize = true;
		((Control)VisC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)VisC).Location = new Point(363, 124);
		((Control)VisC).Name = "VisC";
		((Control)VisC).Size = new Size(92, 29);
		((Control)VisC).TabIndex = 36;
		((ButtonBase)VisC).Text = "Visible";
		((ButtonBase)VisC).UseVisualStyleBackColor = true;
		((ButtonBase)MulC).AutoSize = true;
		((Control)MulC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)MulC).Location = new Point(203, 0);
		((Control)MulC).Name = "MulC";
		((Control)MulC).RightToLeft = (RightToLeft)1;
		((Control)MulC).Size = new Size(252, 29);
		((Control)MulC).TabIndex = 30;
		((ButtonBase)MulC).Text = "Мультикористувацький";
		((ButtonBase)MulC).UseVisualStyleBackColor = true;
		((Control)MulC).Visible = false;
		Label8.AutoSize = true;
		((Control)Label8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label8).Location = new Point(306, 250);
		((Control)Label8).Name = "Label8";
		((Control)Label8).Size = new Size(50, 25);
		((Control)Label8).TabIndex = 35;
		Label8.Text = "Max";
		Label7.AutoSize = true;
		((Control)Label7).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label7).Location = new Point(94, 250);
		((Control)Label7).Name = "Label7";
		((Control)Label7).Size = new Size(44, 25);
		((Control)Label7).TabIndex = 34;
		Label7.Text = "Min";
		Label6.AutoSize = true;
		((Control)Label6).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label6).Location = new Point(20, 209);
		((Control)Label6).Name = "Label6";
		((Control)Label6).Size = new Size(176, 25);
		((Control)Label6).TabIndex = 33;
		Label6.Text = "Резервні номери:";
		((Control)MaxT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)MaxT).Location = new Point(367, 245);
		((Control)MaxT).Name = "MaxT";
		((Control)MaxT).Size = new Size(88, 30);
		((Control)MaxT).TabIndex = 32;
		MaxT.TextAlign = (HorizontalAlignment)2;
		((Control)MinT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)MinT).Location = new Point(159, 245);
		((Control)MinT).Name = "MinT";
		((Control)MinT).Size = new Size(88, 30);
		((Control)MinT).TabIndex = 31;
		MinT.TextAlign = (HorizontalAlignment)2;
		Label5.AutoSize = true;
		((Control)Label5).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label5).Location = new Point(144, 162);
		((Control)Label5).Name = "Label5";
		((Control)Label5).Size = new Size(260, 25);
		((Control)Label5).TabIndex = 30;
		Label5.Text = "Відступ між індикаторами";
		Label4.AutoSize = true;
		((Control)Label4).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label4).Location = new Point(144, 126);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(172, 25);
		((Control)Label4).TabIndex = 29;
		Label4.Text = "Місце індикатора";
		((Control)IndOt).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)IndOt).Location = new Point(25, 159);
		((Control)IndOt).Name = "IndOt";
		((Control)IndOt).Size = new Size(102, 30);
		((Control)IndOt).TabIndex = 28;
		IndOt.TextAlign = (HorizontalAlignment)2;
		((Control)IndYt).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)IndYt).Location = new Point(25, 123);
		((Control)IndYt).Name = "IndYt";
		((Control)IndYt).Size = new Size(102, 30);
		((Control)IndYt).TabIndex = 27;
		IndYt.TextAlign = (HorizontalAlignment)2;
		((ButtonBase)OffAc).AutoSize = true;
		((Control)OffAc).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OffAc).Location = new Point(25, 76);
		((Control)OffAc).Name = "OffAc";
		((Control)OffAc).Size = new Size(325, 29);
		((Control)OffAc).TabIndex = 21;
		((ButtonBase)OffAc).Text = "Автоматичний офлайн режим";
		((ButtonBase)OffAc).UseVisualStyleBackColor = true;
		((ButtonBase)OffC).AutoSize = true;
		((Control)OffC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OffC).Location = new Point(25, 41);
		((Control)OffC).Name = "OffC";
		((Control)OffC).Size = new Size(291, 29);
		((Control)OffC).TabIndex = 20;
		((ButtonBase)OffC).Text = "Дозволити офлайн режим";
		((ButtonBase)OffC).UseVisualStyleBackColor = true;
		((Control)GroupBox2).Controls.Add((Control)(object)Rb80);
		((Control)GroupBox2).Controls.Add((Control)(object)Rb57);
		((Control)GroupBox2).Controls.Add((Control)(object)Label3);
		((Control)GroupBox2).Controls.Add((Control)(object)DlT);
		((Control)GroupBox2).Controls.Add((Control)(object)XmlC);
		((Control)GroupBox2).Controls.Add((Control)(object)LogC);
		((Control)GroupBox2).Controls.Add((Control)(object)TxtC);
		((Control)GroupBox2).Controls.Add((Control)(object)PdfC);
		((Control)GroupBox2).Controls.Add((Control)(object)PrAc);
		((Control)GroupBox2).Controls.Add((Control)(object)PrXc);
		((Control)GroupBox2).Controls.Add((Control)(object)PrC);
		((Control)GroupBox2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox2).Location = new Point(533, 20);
		((Control)GroupBox2).Name = "GroupBox2";
		((Control)GroupBox2).Size = new Size(408, 336);
		((Control)GroupBox2).TabIndex = 21;
		GroupBox2.TabStop = false;
		GroupBox2.Text = "Друк та експорт";
		((ButtonBase)Rb80).AutoSize = true;
		((Control)Rb80).Location = new Point(290, 177);
		((Control)Rb80).Name = "Rb80";
		((Control)Rb80).Size = new Size(94, 29);
		((Control)Rb80).TabIndex = 29;
		Rb80.TabStop = true;
		((ButtonBase)Rb80).Text = "80 мм";
		((ButtonBase)Rb80).UseVisualStyleBackColor = true;
		((ButtonBase)Rb57).AutoSize = true;
		((Control)Rb57).Location = new Point(290, 142);
		((Control)Rb57).Name = "Rb57";
		((Control)Rb57).Size = new Size(94, 29);
		((Control)Rb57).TabIndex = 28;
		Rb57.TabStop = true;
		((ButtonBase)Rb57).Text = "57 мм";
		((ButtonBase)Rb57).UseVisualStyleBackColor = true;
		Label3.AutoSize = true;
		((Control)Label3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label3).Location = new Point(26, 286);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(173, 25);
		((Control)Label3).TabIndex = 27;
		Label3.Text = "Символів в рядку";
		((Control)DlT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)DlT).Location = new Point(228, 281);
		((Control)DlT).Name = "DlT";
		((Control)DlT).Size = new Size(156, 30);
		((Control)DlT).TabIndex = 26;
		DlT.TextAlign = (HorizontalAlignment)2;
		((ButtonBase)XmlC).AutoSize = true;
		((Control)XmlC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)XmlC).Location = new Point(25, 229);
		((Control)XmlC).Name = "XmlC";
		((Control)XmlC).Size = new Size(174, 29);
		((Control)XmlC).TabIndex = 25;
		((ButtonBase)XmlC).Text = "Експорт в XML";
		((ButtonBase)XmlC).UseVisualStyleBackColor = true;
		((ButtonBase)LogC).AutoSize = true;
		((Control)LogC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)LogC).Location = new Point(258, 229);
		((Control)LogC).Name = "LogC";
		((Control)LogC).RightToLeft = (RightToLeft)1;
		((Control)LogC).Size = new Size(126, 29);
		((Control)LogC).TabIndex = 20;
		((ButtonBase)LogC).Text = "Вести лог";
		((ButtonBase)LogC).UseVisualStyleBackColor = true;
		((ButtonBase)TxtC).AutoSize = true;
		((Control)TxtC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TxtC).Location = new Point(25, 194);
		((Control)TxtC).Name = "TxtC";
		((Control)TxtC).Size = new Size(172, 29);
		((Control)TxtC).TabIndex = 24;
		((ButtonBase)TxtC).Text = "Експорт в TXT";
		((ButtonBase)TxtC).UseVisualStyleBackColor = true;
		((ButtonBase)PdfC).AutoSize = true;
		((Control)PdfC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PdfC).Location = new Point(25, 159);
		((Control)PdfC).Name = "PdfC";
		((Control)PdfC).Size = new Size(171, 29);
		((Control)PdfC).TabIndex = 23;
		((ButtonBase)PdfC).Text = "Експорт в PDF";
		((ButtonBase)PdfC).UseVisualStyleBackColor = true;
		((ButtonBase)PrAc).AutoSize = true;
		((Control)PrAc).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PrAc).Location = new Point(25, 111);
		((Control)PrAc).Name = "PrAc";
		((Control)PrAc).Size = new Size(221, 29);
		((Control)PrAc).TabIndex = 22;
		((ButtonBase)PrAc).Text = "Автоматичний друк";
		((ButtonBase)PrAc).UseVisualStyleBackColor = true;
		((ButtonBase)PrXc).AutoSize = true;
		((Control)PrXc).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PrXc).Location = new Point(25, 76);
		((Control)PrXc).Name = "PrXc";
		((Control)PrXc).Size = new Size(342, 29);
		((Control)PrXc).TabIndex = 21;
		((ButtonBase)PrXc).Text = "Показувати форми друку Х звіту";
		((ButtonBase)PrXc).UseVisualStyleBackColor = true;
		((ButtonBase)PrC).AutoSize = true;
		((Control)PrC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)PrC).Location = new Point(25, 41);
		((Control)PrC).Name = "PrC";
		((Control)PrC).Size = new Size(272, 29);
		((Control)PrC).TabIndex = 20;
		((ButtonBase)PrC).Text = "Показувати форми друку";
		((ButtonBase)PrC).UseVisualStyleBackColor = true;
		((Control)FnT).Enabled = false;
		((Control)FnT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)FnT).Location = new Point(107, 16);
		((Control)FnT).Name = "FnT";
		((Control)FnT).Size = new Size(283, 30);
		((Control)FnT).TabIndex = 1;
		FnT.TextAlign = (HorizontalAlignment)2;
		Label2.AutoSize = true;
		((Control)Label2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label2).Location = new Point(12, 21);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(89, 25);
		((Control)Label2).TabIndex = 2;
		Label2.Text = "ПРРО №";
		((ButtonBase)OnC).AutoSize = true;
		((Control)OnC).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OnC).Location = new Point(868, 17);
		((Control)OnC).Name = "OnC";
		((Control)OnC).Size = new Size(64, 29);
		((Control)OnC).TabIndex = 19;
		((ButtonBase)OnC).Text = "ON";
		((ButtonBase)OnC).UseVisualStyleBackColor = true;
		((Control)TinT).Enabled = false;
		((Control)TinT).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TinT).Location = new Point(512, 16);
		((Control)TinT).Name = "TinT";
		((Control)TinT).Size = new Size(315, 30);
		((Control)TinT).TabIndex = 20;
		TinT.TextAlign = (HorizontalAlignment)2;
		Label1.AutoSize = true;
		((Control)Label1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Label1).Location = new Point(438, 21);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(68, 25);
		((Control)Label1).TabIndex = 21;
		Label1.Text = "TIN №";
		((Control)TabControlAll).Controls.Add((Control)(object)TabPage1);
		((Control)TabControlAll).Controls.Add((Control)(object)TabPage2);
		((Control)TabControlAll).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TabControlAll).Location = new Point(18, 133);
		((Control)TabControlAll).Name = "TabControlAll";
		TabControlAll.SelectedIndex = 0;
		((Control)TabControlAll).Size = new Size(976, 515);
		((Control)TabControlAll).TabIndex = 33;
		((Control)TabPage1).Controls.Add((Control)(object)GroupBox3);
		((Control)TabPage1).Controls.Add((Control)(object)GroupBox6);
		((Control)TabPage1).Controls.Add((Control)(object)GroupBox2);
		((Control)TabPage1).Controls.Add((Control)(object)GroupBox4);
		((Control)TabPage1).Font = new Font("Microsoft Sans Serif", 7.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		TabPage1.Location = new Point(4, 34);
		((Control)TabPage1).Name = "TabPage1";
		((Control)TabPage1).Padding = new Padding(3);
		((Control)TabPage1).Size = new Size(968, 477);
		TabPage1.TabIndex = 0;
		TabPage1.Text = "   Основні   ";
		TabPage1.UseVisualStyleBackColor = true;
		((Control)TabPage2).Controls.Add((Control)(object)GroupBox8);
		((Control)TabPage2).Controls.Add((Control)(object)GroupBox7);
		((Control)TabPage2).Controls.Add((Control)(object)GroupBox5);
		TabPage2.Location = new Point(4, 34);
		((Control)TabPage2).Name = "TabPage2";
		((Control)TabPage2).Padding = new Padding(3);
		((Control)TabPage2).Size = new Size(968, 477);
		TabPage2.TabIndex = 1;
		TabPage2.Text = "   Інтеграція   ";
		TabPage2.UseVisualStyleBackColor = true;
		((Control)GroupBox7).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox7).Location = new Point(26, 137);
		((Control)GroupBox7).Name = "GroupBox7";
		((Control)GroupBox7).Size = new Size(915, 182);
		((Control)GroupBox7).TabIndex = 30;
		GroupBox7.TabStop = false;
		GroupBox7.Text = "Налаштування еАкциз";
		((ButtonBase)CBgov).AutoSize = true;
		((Control)CBgov).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CBgov).Location = new Point(38, 43);
		((Control)CBgov).Name = "CBgov";
		((Control)CBgov).Size = new Size(757, 29);
		((Control)CBgov).TabIndex = 29;
		((ButtonBase)CBgov).Text = " Передавати додаткові дані фіскального чека до програми 'Національний чек'";
		((ButtonBase)CBgov).UseVisualStyleBackColor = true;
		((Control)GroupBox8).Controls.Add((Control)(object)CBgov);
		((Control)GroupBox8).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GroupBox8).Location = new Point(26, 21);
		((Control)GroupBox8).Name = "GroupBox8";
		((Control)GroupBox8).Size = new Size(915, 110);
		((Control)GroupBox8).TabIndex = 31;
		GroupBox8.TabStop = false;
		GroupBox8.Text = "Налаштування єЧек";
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1005, 659);
		((Control)this).Controls.Add((Control)(object)TabControlAll);
		((Control)this).Controls.Add((Control)(object)OnC);
		((Control)this).Controls.Add((Control)(object)Label1);
		((Control)this).Controls.Add((Control)(object)TinT);
		((Control)this).Controls.Add((Control)(object)Label2);
		((Control)this).Controls.Add((Control)(object)FnT);
		((Control)this).Controls.Add((Control)(object)GroupBox1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormSettings";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Налаштування ПРРО";
		((Control)GroupBox1).ResumeLayout(false);
		((Control)GroupBox1).PerformLayout();
		((Control)GroupBox6).ResumeLayout(false);
		((Control)GroupBox6).PerformLayout();
		((Control)GroupBox5).ResumeLayout(false);
		((Control)GroupBox5).PerformLayout();
		((Control)GroupBox4).ResumeLayout(false);
		((Control)GroupBox4).PerformLayout();
		((Control)GroupBox3).ResumeLayout(false);
		((Control)GroupBox3).PerformLayout();
		((Control)GroupBox2).ResumeLayout(false);
		((Control)GroupBox2).PerformLayout();
		((Control)TabControlAll).ResumeLayout(false);
		((Control)TabPage1).ResumeLayout(false);
		((Control)TabPage2).ResumeLayout(false);
		((Control)GroupBox8).ResumeLayout(false);
		((Control)GroupBox8).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormSettings_Load(object sender, EventArgs e)
	{
		//IL_000b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0011: Expected O, but got Unknown
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		//IL_0017: Unknown result type (might be due to invalid IL or missing references)
		//IL_002d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_00de: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e5: Expected O, but got Unknown
		//IL_00e7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ec: Unknown result type (might be due to invalid IL or missing references)
		//IL_0103: Unknown result type (might be due to invalid IL or missing references)
		//IL_0108: Unknown result type (might be due to invalid IL or missing references)
		LoadPro();
		ServiceControllerStatus status;
		try
		{
			ServiceController val = new ServiceController("WebcheckPRROBackupService");
			status = val.Status;
			bool num = ((Enum)(ServiceControllerStatus)(ref status)).Equals((object)(ServiceControllerStatus)1);
			status = val.Status;
			if (num | ((Enum)(ServiceControllerStatus)(ref status)).Equals((object)(ServiceControllerStatus)3))
			{
				((Control)LabelService).ForeColor = Color.FromArgb(200, 50, 50);
				LabelService.Text = "Служба WebСheckPRROBackup OFF";
			}
			else
			{
				((Control)LabelService).ForeColor = Color.FromArgb(50, 200, 50);
				LabelService.Text = "Служба WebСheckPRROBackup ON";
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			((Control)LabelService).ForeColor = Color.FromArgb(200, 50, 50);
			LabelService.Text = "Служба WebСheckPRROBackup OFF";
			ProjectData.ClearProjectError();
		}
		try
		{
			ServiceController val2 = new ServiceController("WebСheckPRROManagementService");
			status = val2.Status;
			bool num2 = ((Enum)(ServiceControllerStatus)(ref status)).Equals((object)(ServiceControllerStatus)1);
			status = val2.Status;
			if (num2 | ((Enum)(ServiceControllerStatus)(ref status)).Equals((object)(ServiceControllerStatus)3))
			{
				((Control)LabelService1).ForeColor = Color.FromArgb(200, 50, 50);
				LabelService1.Text = "Служба WebСheckPRROManagement OFF";
			}
			else
			{
				((Control)LabelService1).ForeColor = Color.FromArgb(50, 200, 50);
				LabelService1.Text = "Служба WebСheckPRROManagement ON";
			}
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			((Control)LabelService1).ForeColor = Color.FromArgb(200, 50, 50);
			LabelService1.Text = "Служба WebСheckPRROManagement OFF";
			ProjectData.ClearProjectError();
		}
		string filename = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".ini";
		IniHGB iniHGB = new IniHGB(filename);
		string text = iniHGB.GetString("Upload", "LastUpload").Trim();
		if (Operators.CompareString(text, "", false) != 0)
		{
			text = " last upload:  " + text;
		}
		string text2 = iniHGB.GetString("Upload", "LastError").Trim();
		switch (text2)
		{
		case "-":
			text2 = "waiting upload after error";
			goto case "OK";
		default:
			text2 = "ERROR";
			UpLoadOrder = true;
			goto case "OK";
		case "OK":
			text2 = "   ( " + text2 + " ) ";
			break;
		case null:
		case "":
			break;
		}
		GroupBox5.Text = "Backup" + text + text2;
	}

	private void LoadPro()
	{
		FnT.Text = All.A.FN;
		((Control)GroupBox3).Enabled = All.A.FullVersion;
		switch (All.f.IntegerGetFn(All.A.FN, "PrinterWidth"))
		{
		case 57:
			Rb57.Checked = true;
			break;
		case 80:
			Rb80.Checked = true;
			break;
		default:
			Rb57.Checked = true;
			break;
		}
		try
		{
			CBgov.Checked = All.f.IntegerGetFn(All.A.FN, "useecheckmegovua") != 0;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			CBgov.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			OnC.Checked = All.f.IntegerGetFn(All.A.FN, "On") != 0;
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			OnC.Checked = true;
			ProjectData.ClearProjectError();
		}
		try
		{
			LogC.Checked = All.f.IntegerGetFn(All.A.FN, "LogOn") != 0;
		}
		catch (Exception ex5)
		{
			ProjectData.SetProjectError(ex5);
			Exception ex6 = ex5;
			LogC.Checked = true;
			ProjectData.ClearProjectError();
		}
		try
		{
			PrC.Checked = All.f.IntegerGetFn(All.A.FN, "ShowPintForm") != 0;
		}
		catch (Exception ex7)
		{
			ProjectData.SetProjectError(ex7);
			Exception ex8 = ex7;
			PrC.Checked = true;
			ProjectData.ClearProjectError();
		}
		try
		{
			OffC.Checked = All.f.IntegerGetFn(All.A.FN, "Offline") != 0;
		}
		catch (Exception ex9)
		{
			ProjectData.SetProjectError(ex9);
			Exception ex10 = ex9;
			OffC.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			PrXc.Checked = All.f.IntegerGetFn(All.A.FN, "ShowPintFormX") != 0;
		}
		catch (Exception ex11)
		{
			ProjectData.SetProjectError(ex11);
			Exception ex12 = ex11;
			PrXc.Checked = true;
			ProjectData.ClearProjectError();
		}
		try
		{
			PrAc.Checked = All.f.IntegerGetFn(All.A.FN, "AutomatPrintCheck") != 0;
		}
		catch (Exception ex13)
		{
			ProjectData.SetProjectError(ex13);
			Exception ex14 = ex13;
			PrAc.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			PdfC.Checked = All.f.IntegerGetFn(All.A.FN, "ToPDF") != 0;
		}
		catch (Exception ex15)
		{
			ProjectData.SetProjectError(ex15);
			Exception ex16 = ex15;
			PdfC.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			TxtC.Checked = All.f.IntegerGetFn(All.A.FN, "ToTXT") != 0;
		}
		catch (Exception ex17)
		{
			ProjectData.SetProjectError(ex17);
			Exception ex18 = ex17;
			TxtC.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			XmlC.Checked = All.f.IntegerGetFn(All.A.FN, "ToXML") != 0;
		}
		catch (Exception ex19)
		{
			ProjectData.SetProjectError(ex19);
			Exception ex20 = ex19;
			XmlC.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			VisC.Checked = All.f.IntegerGetFn(All.A.FN, "IndicatorVisible") != 0;
		}
		catch (Exception ex21)
		{
			ProjectData.SetProjectError(ex21);
			Exception ex22 = ex21;
			VisC.Checked = false;
			ProjectData.ClearProjectError();
		}
		if (All.f.IntegerGetFn(All.A.FN, "FiscalMode") > 0)
		{
			RejT.Text = "Увімкнено ФІСКАЛЬНИЙ РЕЖИМ";
			((Control)FisB).Enabled = false;
			((Control)TesB).Enabled = true;
		}
		else
		{
			RejT.Text = "Увімкнено Тестовий режим";
			((Control)TesB).Enabled = false;
			((Control)FisB).Enabled = true;
		}
		try
		{
			OffAc.Checked = All.f.IntegerGetFn(All.A.FN, "AutomatOfflineOn") != 0;
		}
		catch (Exception ex23)
		{
			ProjectData.SetProjectError(ex23);
			Exception ex24 = ex23;
			OffAc.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			AcsC.Checked = All.f.IntegerGetFn(All.A.FN, "UseACSKTSPserver") != 0;
		}
		catch (Exception ex25)
		{
			ProjectData.SetProjectError(ex25);
			Exception ex26 = ex25;
			AcsC.Checked = false;
			ProjectData.ClearProjectError();
		}
		try
		{
			MulC.Checked = All.f.IntegerGetFn(All.A.FN, "Multiplayer") != 0;
		}
		catch (Exception ex27)
		{
			ProjectData.SetProjectError(ex27);
			Exception ex28 = ex27;
			MulC.Checked = true;
			ProjectData.ClearProjectError();
		}
		Server.Text = All.SF.Servers(All.f.IntegerGetFn(All.A.FN, "Acsksettings")).Name;
		MinT.Text = All.f.IntegerGetFn(All.A.FN, "OfflineMin").ToString();
		MaxT.Text = All.f.IntegerGetFn(All.A.FN, "OfflineMax").ToString();
		DlT.Text = All.f.IntegerGetFn(All.A.FN, "ExportLength").ToString();
		TinT.Text = All.f.StringGetFn(All.A.FN, "TIN");
		IndYt.Text = All.f.StringGetFn(All.A.FN, "IndicatorY");
		IndOt.Text = All.f.StringGetFn(All.A.FN, "IndicatorStepY");
		((Control)GroupBox1).Enabled = OnC.Checked;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		if (!File.Exists(text) && !All.l.TableKsef())
		{
			FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
		if (!File.Exists(text))
		{
			((Control)BackupB).Enabled = true;
			fText.Text = "вимкнено";
			lText.Text = "вимкнено";
		}
		else
		{
			((Control)BackupB).Enabled = false;
			GetInfoBackup();
		}
		TimeIni();
	}

	private void TimeIni()
	{
		string text = All.f.StringGetFn(All.A.FN, "shiftclosetime");
		string text2;
		if (Versioned.IsNumeric((object)text))
		{
			TimeSpan timeSpan = TimeSpan.FromMinutes(Conversions.ToDouble(text));
			if (timeSpan.Days > 0)
			{
				text2 = "вимкнено";
			}
			else
			{
				text2 = timeSpan.ToString("hh\\:mm");
				text2 = ((All.f.IntegerGetFn(All.A.FN, "shiftCashInOut") != 1) ? (text2 + "  без сл.видача") : (text2 + "  з сл.видача"));
			}
		}
		else
		{
			text2 = "вимкнено";
		}
		TBT.Text = text2;
	}

	private void GetInfoBackup()
	{
		TypBackupInfo typBackupInfo = All.l.InfoBackup();
		fText.Text = typBackupInfo.First;
		lText.Text = typBackupInfo.Last;
	}

	private void OnC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "On", Math.Abs(0 - (OnC.Checked ? 1 : 0)));
		((Control)GroupBox1).Enabled = OnC.Checked;
	}

	private void LogC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "LogOn", Math.Abs(0 - (LogC.Checked ? 1 : 0)));
	}

	private void PrC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "ShowPintForm", Math.Abs(0 - (PrC.Checked ? 1 : 0)));
	}

	private void PrXc_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "ShowPintFormX", Math.Abs(0 - (PrXc.Checked ? 1 : 0)));
	}

	private void PrAc_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "AutomatPrintCheck", Math.Abs(0 - (PrAc.Checked ? 1 : 0)));
	}

	private void PdfC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "ToPDF", Math.Abs(0 - (PdfC.Checked ? 1 : 0)));
	}

	private void TxtC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "ToTXT", Math.Abs(0 - (TxtC.Checked ? 1 : 0)));
	}

	private void XmlC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "ToXML", Math.Abs(0 - (XmlC.Checked ? 1 : 0)));
	}

	private void DlT_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)DlT.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "ExportLength", Conversions.ToInteger(DlT.Text));
		}
	}

	private void OffC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "Offline", Math.Abs(0 - (OffC.Checked ? 1 : 0)));
	}

	private void OffAc_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "AutomatOfflineOn", Math.Abs(0 - (OffAc.Checked ? 1 : 0)));
	}

	private void MailB_Click(object sender, EventArgs e)
	{
		//IL_0005: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormCloseShift()).ShowDialog();
		TimeIni();
	}

	private void FormSettings_Closing(object sender, CancelEventArgs e)
	{
		try
		{
			if (((Control)BackupB).Enabled && UpLoadOrder)
			{
				string filename = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".ini";
				IniHGB iniHGB = new IniHGB(filename);
				iniHGB.WriteString("Upload", "DateError", "");
				iniHGB.WriteString("Upload", "LastError", "-");
				iniHGB.WriteString("Upload", "Z", "9");
				iniHGB.WriteString("Upload", "LastOrder", DateTime.Now.ToString());
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void IndYt_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)IndYt.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "IndicatorY", Conversions.ToInteger(IndYt.Text));
		}
	}

	private void IndOt_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)IndOt.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "IndicatorStepY", Conversions.ToInteger(IndOt.Text));
		}
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		((Form)new FormServerSelection(NewBase: false)).ShowDialog();
		Server.Text = All.SF.Servers(All.f.IntegerGetFn(All.A.FN, "Acsksettings")).Name;
	}

	private void AcsC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "UseACSKTSPserver", Math.Abs(0 - (AcsC.Checked ? 1 : 0)));
	}

	private void TesB_Click(object sender, EventArgs e)
	{
		RejT.Text = "Увімкнено Тестовий режим";
		((Control)TesB).Enabled = false;
		((Control)FisB).Enabled = true;
		All.f.IntigerWriteFN(All.A.FN, "FiscalMode", 0);
	}

	private void FisB_Click(object sender, EventArgs e)
	{
		RejT.Text = "Увімкнено ФІСКАЛЬНИЙ РЕЖИМ";
		((Control)FisB).Enabled = false;
		((Control)TesB).Enabled = true;
		All.f.IntigerWriteFN(All.A.FN, "FiscalMode", 1);
	}

	private void MinT_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)MinT.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "OfflineMin", Conversions.ToInteger(MinT.Text));
		}
	}

	private void MaxT_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric((object)MaxT.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "OfflineMax", Conversions.ToInteger(MaxT.Text));
		}
	}

	private void VisC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "IndicatorVisible", Math.Abs(0 - (VisC.Checked ? 1 : 0)));
	}

	private void Rb57_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "PrinterWidth", 57);
	}

	private void Rb80_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "PrinterWidth", 80);
	}

	private void MulC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "Multiplayer", Math.Abs(0 - (MulC.Checked ? 1 : 0)));
	}

	private void BackupB_Click(object sender, EventArgs e)
	{
		//IL_0023: Unknown result type (might be due to invalid IL or missing references)
		//IL_00aa: Unknown result type (might be due to invalid IL or missing references)
		((Control)BackupB).Enabled = false;
		if (!All.A.FullVersion)
		{
			Interaction.MsgBox((object)"Ведення резервної бази доступне лише у повній версії!", (MsgBoxStyle)0, (object)"Backup");
			((Control)BackupB).Enabled = true;
			return;
		}
		CreateDB createDB = new CreateDB(All.A.FN);
		createDB.CreateTable(13);
		createDB.CreateTrigerBackup();
		string fileN = All.A.FileN;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		try
		{
			if (!File.Exists(text))
			{
				File.Copy(fileN, text);
				Application.DoEvents();
				All.l.ClearBackups();
				Application.DoEvents();
			}
			Interaction.MsgBox((object)"Ведення резервної распочато!", (MsgBoxStyle)0, (object)"Backup");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		if (!File.Exists(text))
		{
			((Control)BackupB).Enabled = true;
			fText.Text = "вимкнено";
			lText.Text = "вимкнено";
		}
		else
		{
			((Control)BackupB).Enabled = false;
			GetInfoBackup();
		}
	}

	private void FormSettings_FormClosing(object sender, FormClosingEventArgs e)
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		if (!File.Exists(text) && !All.l.TableKsef())
		{
			FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
	}

	private void CBgov_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "useecheckmegovua", Math.Abs(0 - (CBgov.Checked ? 1 : 0)));
	}
}
