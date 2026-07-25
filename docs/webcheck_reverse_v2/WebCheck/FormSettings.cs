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
			EventHandler value2 = OnC_CheckedChanged;
			CheckBox onC = _OnC;
			if (onC != null)
			{
				onC.CheckedChanged -= value2;
			}
			_OnC = value;
			onC = _OnC;
			if (onC != null)
			{
				onC.CheckedChanged += value2;
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
			EventHandler value2 = LogC_CheckedChanged;
			CheckBox logC = _LogC;
			if (logC != null)
			{
				logC.CheckedChanged -= value2;
			}
			_LogC = value;
			logC = _LogC;
			if (logC != null)
			{
				logC.CheckedChanged += value2;
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
			EventHandler value2 = PrC_CheckedChanged;
			CheckBox prC = _PrC;
			if (prC != null)
			{
				prC.CheckedChanged -= value2;
			}
			_PrC = value;
			prC = _PrC;
			if (prC != null)
			{
				prC.CheckedChanged += value2;
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
			EventHandler value2 = PrXc_CheckedChanged;
			CheckBox prXc = _PrXc;
			if (prXc != null)
			{
				prXc.CheckedChanged -= value2;
			}
			_PrXc = value;
			prXc = _PrXc;
			if (prXc != null)
			{
				prXc.CheckedChanged += value2;
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
			EventHandler value2 = PrAc_CheckedChanged;
			CheckBox prAc = _PrAc;
			if (prAc != null)
			{
				prAc.CheckedChanged -= value2;
			}
			_PrAc = value;
			prAc = _PrAc;
			if (prAc != null)
			{
				prAc.CheckedChanged += value2;
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
			EventHandler value2 = OffAc_CheckedChanged;
			CheckBox offAc = _OffAc;
			if (offAc != null)
			{
				offAc.CheckedChanged -= value2;
			}
			_OffAc = value;
			offAc = _OffAc;
			if (offAc != null)
			{
				offAc.CheckedChanged += value2;
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
			EventHandler value2 = OffC_CheckedChanged;
			CheckBox offC = _OffC;
			if (offC != null)
			{
				offC.CheckedChanged -= value2;
			}
			_OffC = value;
			offC = _OffC;
			if (offC != null)
			{
				offC.CheckedChanged += value2;
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
			EventHandler value2 = XmlC_CheckedChanged;
			CheckBox xmlC = _XmlC;
			if (xmlC != null)
			{
				xmlC.CheckedChanged -= value2;
			}
			_XmlC = value;
			xmlC = _XmlC;
			if (xmlC != null)
			{
				xmlC.CheckedChanged += value2;
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
			EventHandler value2 = TxtC_CheckedChanged;
			CheckBox txtC = _TxtC;
			if (txtC != null)
			{
				txtC.CheckedChanged -= value2;
			}
			_TxtC = value;
			txtC = _TxtC;
			if (txtC != null)
			{
				txtC.CheckedChanged += value2;
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
			EventHandler value2 = PdfC_CheckedChanged;
			CheckBox pdfC = _PdfC;
			if (pdfC != null)
			{
				pdfC.CheckedChanged -= value2;
			}
			_PdfC = value;
			pdfC = _PdfC;
			if (pdfC != null)
			{
				pdfC.CheckedChanged += value2;
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
			EventHandler value2 = DlT_TextChanged;
			TextBox dlT = _DlT;
			if (dlT != null)
			{
				dlT.TextChanged -= value2;
			}
			_DlT = value;
			dlT = _DlT;
			if (dlT != null)
			{
				dlT.TextChanged += value2;
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
			EventHandler value2 = MailB_Click;
			Button mailB = _MailB;
			if (mailB != null)
			{
				mailB.Click -= value2;
			}
			_MailB = value;
			mailB = _MailB;
			if (mailB != null)
			{
				mailB.Click += value2;
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
			EventHandler value2 = IndOt_TextChanged;
			TextBox indOt = _IndOt;
			if (indOt != null)
			{
				indOt.TextChanged -= value2;
			}
			_IndOt = value;
			indOt = _IndOt;
			if (indOt != null)
			{
				indOt.TextChanged += value2;
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
			EventHandler value2 = IndYt_TextChanged;
			TextBox indYt = _IndYt;
			if (indYt != null)
			{
				indYt.TextChanged -= value2;
			}
			_IndYt = value;
			indYt = _IndYt;
			if (indYt != null)
			{
				indYt.TextChanged += value2;
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
			EventHandler value2 = SelSwrver_Click;
			Button selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click -= value2;
			}
			_SelSwrver = value;
			selSwrver = _SelSwrver;
			if (selSwrver != null)
			{
				selSwrver.Click += value2;
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
			EventHandler value2 = AcsC_CheckedChanged;
			CheckBox acsC = _AcsC;
			if (acsC != null)
			{
				acsC.CheckedChanged -= value2;
			}
			_AcsC = value;
			acsC = _AcsC;
			if (acsC != null)
			{
				acsC.CheckedChanged += value2;
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
			EventHandler value2 = TesB_Click;
			Button tesB = _TesB;
			if (tesB != null)
			{
				tesB.Click -= value2;
			}
			_TesB = value;
			tesB = _TesB;
			if (tesB != null)
			{
				tesB.Click += value2;
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
			EventHandler value2 = FisB_Click;
			Button fisB = _FisB;
			if (fisB != null)
			{
				fisB.Click -= value2;
			}
			_FisB = value;
			fisB = _FisB;
			if (fisB != null)
			{
				fisB.Click += value2;
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
			EventHandler value2 = MaxT_TextChanged;
			TextBox maxT = _MaxT;
			if (maxT != null)
			{
				maxT.TextChanged -= value2;
			}
			_MaxT = value;
			maxT = _MaxT;
			if (maxT != null)
			{
				maxT.TextChanged += value2;
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
			EventHandler value2 = MinT_TextChanged;
			TextBox minT = _MinT;
			if (minT != null)
			{
				minT.TextChanged -= value2;
			}
			_MinT = value;
			minT = _MinT;
			if (minT != null)
			{
				minT.TextChanged += value2;
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
			EventHandler value2 = VisC_CheckedChanged;
			CheckBox visC = _VisC;
			if (visC != null)
			{
				visC.CheckedChanged -= value2;
			}
			_VisC = value;
			visC = _VisC;
			if (visC != null)
			{
				visC.CheckedChanged += value2;
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
			EventHandler value2 = Rb80_CheckedChanged;
			RadioButton rb = _Rb80;
			if (rb != null)
			{
				rb.CheckedChanged -= value2;
			}
			_Rb80 = value;
			rb = _Rb80;
			if (rb != null)
			{
				rb.CheckedChanged += value2;
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
			EventHandler value2 = Rb57_CheckedChanged;
			RadioButton rb = _Rb57;
			if (rb != null)
			{
				rb.CheckedChanged -= value2;
			}
			_Rb57 = value;
			rb = _Rb57;
			if (rb != null)
			{
				rb.CheckedChanged += value2;
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
			EventHandler value2 = MulC_CheckedChanged;
			CheckBox mulC = _MulC;
			if (mulC != null)
			{
				mulC.CheckedChanged -= value2;
			}
			_MulC = value;
			mulC = _MulC;
			if (mulC != null)
			{
				mulC.CheckedChanged += value2;
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
			EventHandler value2 = BackupB_Click;
			Button backupB = _BackupB;
			if (backupB != null)
			{
				backupB.Click -= value2;
			}
			_BackupB = value;
			backupB = _BackupB;
			if (backupB != null)
			{
				backupB.Click += value2;
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
			EventHandler value2 = CBgov_CheckedChanged;
			CheckBox cBgov = _CBgov;
			if (cBgov != null)
			{
				cBgov.CheckedChanged -= value2;
			}
			_CBgov = value;
			cBgov = _CBgov;
			if (cBgov != null)
			{
				cBgov.CheckedChanged += value2;
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
		base.Load += FormSettings_Load;
		base.Closing += FormSettings_Closing;
		base.FormClosing += FormSettings_FormClosing;
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
			base.Dispose(disposing);
		}
	}

	[System.Diagnostics.DebuggerStepThrough]
	private void InitializeComponent()
	{
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormSettings));
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.FisB = new System.Windows.Forms.Button();
		this.TesB = new System.Windows.Forms.Button();
		this.RejT = new System.Windows.Forms.TextBox();
		this.GroupBox6 = new System.Windows.Forms.GroupBox();
		this.TBT = new System.Windows.Forms.TextBox();
		this.MailB = new System.Windows.Forms.Button();
		this.GroupBox5 = new System.Windows.Forms.GroupBox();
		this.LabelService1 = new System.Windows.Forms.Label();
		this.lText = new System.Windows.Forms.TextBox();
		this.LabelService = new System.Windows.Forms.Label();
		this.Label9 = new System.Windows.Forms.Label();
		this.Label10 = new System.Windows.Forms.Label();
		this.fText = new System.Windows.Forms.TextBox();
		this.BackupB = new System.Windows.Forms.Button();
		this.GroupBox4 = new System.Windows.Forms.GroupBox();
		this.AcsC = new System.Windows.Forms.CheckBox();
		this.Server = new System.Windows.Forms.TextBox();
		this.SelSwrver = new System.Windows.Forms.Button();
		this.Label21 = new System.Windows.Forms.Label();
		this.GroupBox3 = new System.Windows.Forms.GroupBox();
		this.VisC = new System.Windows.Forms.CheckBox();
		this.MulC = new System.Windows.Forms.CheckBox();
		this.Label8 = new System.Windows.Forms.Label();
		this.Label7 = new System.Windows.Forms.Label();
		this.Label6 = new System.Windows.Forms.Label();
		this.MaxT = new System.Windows.Forms.TextBox();
		this.MinT = new System.Windows.Forms.TextBox();
		this.Label5 = new System.Windows.Forms.Label();
		this.Label4 = new System.Windows.Forms.Label();
		this.IndOt = new System.Windows.Forms.TextBox();
		this.IndYt = new System.Windows.Forms.TextBox();
		this.OffAc = new System.Windows.Forms.CheckBox();
		this.OffC = new System.Windows.Forms.CheckBox();
		this.GroupBox2 = new System.Windows.Forms.GroupBox();
		this.Rb80 = new System.Windows.Forms.RadioButton();
		this.Rb57 = new System.Windows.Forms.RadioButton();
		this.Label3 = new System.Windows.Forms.Label();
		this.DlT = new System.Windows.Forms.TextBox();
		this.XmlC = new System.Windows.Forms.CheckBox();
		this.LogC = new System.Windows.Forms.CheckBox();
		this.TxtC = new System.Windows.Forms.CheckBox();
		this.PdfC = new System.Windows.Forms.CheckBox();
		this.PrAc = new System.Windows.Forms.CheckBox();
		this.PrXc = new System.Windows.Forms.CheckBox();
		this.PrC = new System.Windows.Forms.CheckBox();
		this.FnT = new System.Windows.Forms.TextBox();
		this.Label2 = new System.Windows.Forms.Label();
		this.OnC = new System.Windows.Forms.CheckBox();
		this.TinT = new System.Windows.Forms.TextBox();
		this.Label1 = new System.Windows.Forms.Label();
		this.TabControlAll = new System.Windows.Forms.TabControl();
		this.TabPage1 = new System.Windows.Forms.TabPage();
		this.TabPage2 = new System.Windows.Forms.TabPage();
		this.GroupBox7 = new System.Windows.Forms.GroupBox();
		this.CBgov = new System.Windows.Forms.CheckBox();
		this.GroupBox8 = new System.Windows.Forms.GroupBox();
		this.GroupBox1.SuspendLayout();
		this.GroupBox6.SuspendLayout();
		this.GroupBox5.SuspendLayout();
		this.GroupBox4.SuspendLayout();
		this.GroupBox3.SuspendLayout();
		this.GroupBox2.SuspendLayout();
		this.TabControlAll.SuspendLayout();
		this.TabPage1.SuspendLayout();
		this.TabPage2.SuspendLayout();
		this.GroupBox8.SuspendLayout();
		base.SuspendLayout();
		this.GroupBox1.Controls.Add(this.FisB);
		this.GroupBox1.Controls.Add(this.TesB);
		this.GroupBox1.Controls.Add(this.RejT);
		this.GroupBox1.Location = new System.Drawing.Point(12, 52);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Size = new System.Drawing.Size(951, 75);
		this.GroupBox1.TabIndex = 0;
		this.GroupBox1.TabStop = false;
		this.FisB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FisB.Location = new System.Drawing.Point(749, 27);
		this.FisB.Name = "FisB";
		this.FisB.Size = new System.Drawing.Size(186, 35);
		this.FisB.TabIndex = 27;
		this.FisB.Text = "Фіскальний";
		this.FisB.UseVisualStyleBackColor = true;
		this.TesB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TesB.Location = new System.Drawing.Point(20, 27);
		this.TesB.Name = "TesB";
		this.TesB.Size = new System.Drawing.Size(186, 35);
		this.TesB.TabIndex = 26;
		this.TesB.Text = "Тестовий";
		this.TesB.UseVisualStyleBackColor = true;
		this.RejT.Font = new System.Drawing.Font("Microsoft Sans Serif", 13.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.RejT.ForeColor = System.Drawing.Color.Black;
		this.RejT.Location = new System.Drawing.Point(246, 26);
		this.RejT.Name = "RejT";
		this.RejT.ReadOnly = true;
		this.RejT.Size = new System.Drawing.Size(462, 34);
		this.RejT.TabIndex = 24;
		this.RejT.TabStop = false;
		this.RejT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.GroupBox6.Controls.Add(this.TBT);
		this.GroupBox6.Controls.Add(this.MailB);
		this.GroupBox6.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox6.Location = new System.Drawing.Point(533, 362);
		this.GroupBox6.Name = "GroupBox6";
		this.GroupBox6.Size = new System.Drawing.Size(408, 95);
		this.GroupBox6.TabIndex = 29;
		this.GroupBox6.TabStop = false;
		this.GroupBox6.Text = "Налаштування закриття зміни";
		this.TBT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TBT.Location = new System.Drawing.Point(25, 42);
		this.TBT.Name = "TBT";
		this.TBT.ReadOnly = true;
		this.TBT.Size = new System.Drawing.Size(263, 30);
		this.TBT.TabIndex = 26;
		this.TBT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.MailB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.MailB.Location = new System.Drawing.Point(307, 42);
		this.MailB.Name = "MailB";
		this.MailB.Size = new System.Drawing.Size(86, 27);
		this.MailB.TabIndex = 23;
		this.MailB.Text = "...";
		this.MailB.UseVisualStyleBackColor = true;
		this.GroupBox5.Controls.Add(this.LabelService1);
		this.GroupBox5.Controls.Add(this.lText);
		this.GroupBox5.Controls.Add(this.LabelService);
		this.GroupBox5.Controls.Add(this.Label9);
		this.GroupBox5.Controls.Add(this.Label10);
		this.GroupBox5.Controls.Add(this.fText);
		this.GroupBox5.Controls.Add(this.BackupB);
		this.GroupBox5.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox5.Location = new System.Drawing.Point(26, 325);
		this.GroupBox5.Name = "GroupBox5";
		this.GroupBox5.Size = new System.Drawing.Size(915, 136);
		this.GroupBox5.TabIndex = 28;
		this.GroupBox5.TabStop = false;
		this.GroupBox5.Text = "Backup";
		this.LabelService1.AutoSize = true;
		this.LabelService1.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LabelService1.Location = new System.Drawing.Point(503, 20);
		this.LabelService1.Name = "LabelService1";
		this.LabelService1.Size = new System.Drawing.Size(340, 24);
		this.LabelService1.TabIndex = 33;
		this.LabelService1.Text = "Служба WebСheckPRROManagement";
		this.lText.Enabled = false;
		this.lText.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.lText.Location = new System.Drawing.Point(168, 92);
		this.lText.Name = "lText";
		this.lText.ReadOnly = true;
		this.lText.Size = new System.Drawing.Size(317, 30);
		this.lText.TabIndex = 30;
		this.lText.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.LabelService.AutoSize = true;
		this.LabelService.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LabelService.Location = new System.Drawing.Point(503, 49);
		this.LabelService.Name = "LabelService";
		this.LabelService.Size = new System.Drawing.Size(292, 24);
		this.LabelService.TabIndex = 32;
		this.LabelService.Text = "Служба WebСheckPRROBackup";
		this.Label9.AutoSize = true;
		this.Label9.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label9.Location = new System.Drawing.Point(10, 48);
		this.Label9.Name = "Label9";
		this.Label9.Size = new System.Drawing.Size(138, 24);
		this.Label9.TabIndex = 31;
		this.Label9.Text = "Перший запис";
		this.Label10.AutoSize = true;
		this.Label10.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label10.Location = new System.Drawing.Point(10, 97);
		this.Label10.Name = "Label10";
		this.Label10.Size = new System.Drawing.Size(149, 24);
		this.Label10.TabIndex = 32;
		this.Label10.Text = "Останній запис";
		this.fText.Enabled = false;
		this.fText.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.fText.Location = new System.Drawing.Point(165, 43);
		this.fText.Name = "fText";
		this.fText.ReadOnly = true;
		this.fText.Size = new System.Drawing.Size(317, 30);
		this.fText.TabIndex = 29;
		this.fText.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.BackupB.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.BackupB.Location = new System.Drawing.Point(534, 91);
		this.BackupB.Name = "BackupB";
		this.BackupB.Size = new System.Drawing.Size(353, 35);
		this.BackupB.TabIndex = 28;
		this.BackupB.Text = "Увімкнути";
		this.BackupB.UseVisualStyleBackColor = true;
		this.GroupBox4.Controls.Add(this.AcsC);
		this.GroupBox4.Controls.Add(this.Server);
		this.GroupBox4.Controls.Add(this.SelSwrver);
		this.GroupBox4.Controls.Add(this.Label21);
		this.GroupBox4.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox4.Location = new System.Drawing.Point(24, 318);
		this.GroupBox4.Name = "GroupBox4";
		this.GroupBox4.Size = new System.Drawing.Size(486, 142);
		this.GroupBox4.TabIndex = 23;
		this.GroupBox4.TabStop = false;
		this.GroupBox4.Text = "Підпис та відправка";
		this.AcsC.AutoSize = true;
		this.AcsC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.AcsC.Location = new System.Drawing.Point(82, 97);
		this.AcsC.Name = "AcsC";
		this.AcsC.Size = new System.Drawing.Size(297, 29);
		this.AcsC.TabIndex = 24;
		this.AcsC.Text = "Використовувати ACSKTSP";
		this.AcsC.UseVisualStyleBackColor = true;
		this.Server.Enabled = false;
		this.Server.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Server.Location = new System.Drawing.Point(97, 47);
		this.Server.Name = "Server";
		this.Server.Size = new System.Drawing.Size(253, 30);
		this.Server.TabIndex = 23;
		this.Server.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.SelSwrver.Font = new System.Drawing.Font("Microsoft Sans Serif", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SelSwrver.Location = new System.Drawing.Point(375, 47);
		this.SelSwrver.Name = "SelSwrver";
		this.SelSwrver.Size = new System.Drawing.Size(86, 30);
		this.SelSwrver.TabIndex = 22;
		this.SelSwrver.Text = "...";
		this.SelSwrver.UseVisualStyleBackColor = true;
		this.Label21.AutoSize = true;
		this.Label21.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label21.Location = new System.Drawing.Point(6, 50);
		this.Label21.Name = "Label21";
		this.Label21.Size = new System.Drawing.Size(64, 25);
		this.Label21.TabIndex = 21;
		this.Label21.Text = "АЦСК";
		this.GroupBox3.Controls.Add(this.VisC);
		this.GroupBox3.Controls.Add(this.MulC);
		this.GroupBox3.Controls.Add(this.Label8);
		this.GroupBox3.Controls.Add(this.Label7);
		this.GroupBox3.Controls.Add(this.Label6);
		this.GroupBox3.Controls.Add(this.MaxT);
		this.GroupBox3.Controls.Add(this.MinT);
		this.GroupBox3.Controls.Add(this.Label5);
		this.GroupBox3.Controls.Add(this.Label4);
		this.GroupBox3.Controls.Add(this.IndOt);
		this.GroupBox3.Controls.Add(this.IndYt);
		this.GroupBox3.Controls.Add(this.OffAc);
		this.GroupBox3.Controls.Add(this.OffC);
		this.GroupBox3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox3.Location = new System.Drawing.Point(24, 20);
		this.GroupBox3.Name = "GroupBox3";
		this.GroupBox3.Size = new System.Drawing.Size(486, 292);
		this.GroupBox3.TabIndex = 22;
		this.GroupBox3.TabStop = false;
		this.GroupBox3.Text = "Офлайн";
		this.VisC.AutoSize = true;
		this.VisC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.VisC.Location = new System.Drawing.Point(363, 124);
		this.VisC.Name = "VisC";
		this.VisC.Size = new System.Drawing.Size(92, 29);
		this.VisC.TabIndex = 36;
		this.VisC.Text = "Visible";
		this.VisC.UseVisualStyleBackColor = true;
		this.MulC.AutoSize = true;
		this.MulC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.MulC.Location = new System.Drawing.Point(203, 0);
		this.MulC.Name = "MulC";
		this.MulC.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
		this.MulC.Size = new System.Drawing.Size(252, 29);
		this.MulC.TabIndex = 30;
		this.MulC.Text = "Мультикористувацький";
		this.MulC.UseVisualStyleBackColor = true;
		this.MulC.Visible = false;
		this.Label8.AutoSize = true;
		this.Label8.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label8.Location = new System.Drawing.Point(306, 250);
		this.Label8.Name = "Label8";
		this.Label8.Size = new System.Drawing.Size(50, 25);
		this.Label8.TabIndex = 35;
		this.Label8.Text = "Max";
		this.Label7.AutoSize = true;
		this.Label7.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label7.Location = new System.Drawing.Point(94, 250);
		this.Label7.Name = "Label7";
		this.Label7.Size = new System.Drawing.Size(44, 25);
		this.Label7.TabIndex = 34;
		this.Label7.Text = "Min";
		this.Label6.AutoSize = true;
		this.Label6.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label6.Location = new System.Drawing.Point(20, 209);
		this.Label6.Name = "Label6";
		this.Label6.Size = new System.Drawing.Size(176, 25);
		this.Label6.TabIndex = 33;
		this.Label6.Text = "Резервні номери:";
		this.MaxT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.MaxT.Location = new System.Drawing.Point(367, 245);
		this.MaxT.Name = "MaxT";
		this.MaxT.Size = new System.Drawing.Size(88, 30);
		this.MaxT.TabIndex = 32;
		this.MaxT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.MinT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.MinT.Location = new System.Drawing.Point(159, 245);
		this.MinT.Name = "MinT";
		this.MinT.Size = new System.Drawing.Size(88, 30);
		this.MinT.TabIndex = 31;
		this.MinT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label5.AutoSize = true;
		this.Label5.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label5.Location = new System.Drawing.Point(144, 162);
		this.Label5.Name = "Label5";
		this.Label5.Size = new System.Drawing.Size(260, 25);
		this.Label5.TabIndex = 30;
		this.Label5.Text = "Відступ між індикаторами";
		this.Label4.AutoSize = true;
		this.Label4.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label4.Location = new System.Drawing.Point(144, 126);
		this.Label4.Name = "Label4";
		this.Label4.Size = new System.Drawing.Size(172, 25);
		this.Label4.TabIndex = 29;
		this.Label4.Text = "Місце індикатора";
		this.IndOt.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.IndOt.Location = new System.Drawing.Point(25, 159);
		this.IndOt.Name = "IndOt";
		this.IndOt.Size = new System.Drawing.Size(102, 30);
		this.IndOt.TabIndex = 28;
		this.IndOt.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.IndYt.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.IndYt.Location = new System.Drawing.Point(25, 123);
		this.IndYt.Name = "IndYt";
		this.IndYt.Size = new System.Drawing.Size(102, 30);
		this.IndYt.TabIndex = 27;
		this.IndYt.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.OffAc.AutoSize = true;
		this.OffAc.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OffAc.Location = new System.Drawing.Point(25, 76);
		this.OffAc.Name = "OffAc";
		this.OffAc.Size = new System.Drawing.Size(325, 29);
		this.OffAc.TabIndex = 21;
		this.OffAc.Text = "Автоматичний офлайн режим";
		this.OffAc.UseVisualStyleBackColor = true;
		this.OffC.AutoSize = true;
		this.OffC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OffC.Location = new System.Drawing.Point(25, 41);
		this.OffC.Name = "OffC";
		this.OffC.Size = new System.Drawing.Size(291, 29);
		this.OffC.TabIndex = 20;
		this.OffC.Text = "Дозволити офлайн режим";
		this.OffC.UseVisualStyleBackColor = true;
		this.GroupBox2.Controls.Add(this.Rb80);
		this.GroupBox2.Controls.Add(this.Rb57);
		this.GroupBox2.Controls.Add(this.Label3);
		this.GroupBox2.Controls.Add(this.DlT);
		this.GroupBox2.Controls.Add(this.XmlC);
		this.GroupBox2.Controls.Add(this.LogC);
		this.GroupBox2.Controls.Add(this.TxtC);
		this.GroupBox2.Controls.Add(this.PdfC);
		this.GroupBox2.Controls.Add(this.PrAc);
		this.GroupBox2.Controls.Add(this.PrXc);
		this.GroupBox2.Controls.Add(this.PrC);
		this.GroupBox2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox2.Location = new System.Drawing.Point(533, 20);
		this.GroupBox2.Name = "GroupBox2";
		this.GroupBox2.Size = new System.Drawing.Size(408, 336);
		this.GroupBox2.TabIndex = 21;
		this.GroupBox2.TabStop = false;
		this.GroupBox2.Text = "Друк та експорт";
		this.Rb80.AutoSize = true;
		this.Rb80.Location = new System.Drawing.Point(290, 177);
		this.Rb80.Name = "Rb80";
		this.Rb80.Size = new System.Drawing.Size(94, 29);
		this.Rb80.TabIndex = 29;
		this.Rb80.TabStop = true;
		this.Rb80.Text = "80 мм";
		this.Rb80.UseVisualStyleBackColor = true;
		this.Rb57.AutoSize = true;
		this.Rb57.Location = new System.Drawing.Point(290, 142);
		this.Rb57.Name = "Rb57";
		this.Rb57.Size = new System.Drawing.Size(94, 29);
		this.Rb57.TabIndex = 28;
		this.Rb57.TabStop = true;
		this.Rb57.Text = "57 мм";
		this.Rb57.UseVisualStyleBackColor = true;
		this.Label3.AutoSize = true;
		this.Label3.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label3.Location = new System.Drawing.Point(26, 286);
		this.Label3.Name = "Label3";
		this.Label3.Size = new System.Drawing.Size(173, 25);
		this.Label3.TabIndex = 27;
		this.Label3.Text = "Символів в рядку";
		this.DlT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.DlT.Location = new System.Drawing.Point(228, 281);
		this.DlT.Name = "DlT";
		this.DlT.Size = new System.Drawing.Size(156, 30);
		this.DlT.TabIndex = 26;
		this.DlT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.XmlC.AutoSize = true;
		this.XmlC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.XmlC.Location = new System.Drawing.Point(25, 229);
		this.XmlC.Name = "XmlC";
		this.XmlC.Size = new System.Drawing.Size(174, 29);
		this.XmlC.TabIndex = 25;
		this.XmlC.Text = "Експорт в XML";
		this.XmlC.UseVisualStyleBackColor = true;
		this.LogC.AutoSize = true;
		this.LogC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LogC.Location = new System.Drawing.Point(258, 229);
		this.LogC.Name = "LogC";
		this.LogC.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
		this.LogC.Size = new System.Drawing.Size(126, 29);
		this.LogC.TabIndex = 20;
		this.LogC.Text = "Вести лог";
		this.LogC.UseVisualStyleBackColor = true;
		this.TxtC.AutoSize = true;
		this.TxtC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TxtC.Location = new System.Drawing.Point(25, 194);
		this.TxtC.Name = "TxtC";
		this.TxtC.Size = new System.Drawing.Size(172, 29);
		this.TxtC.TabIndex = 24;
		this.TxtC.Text = "Експорт в TXT";
		this.TxtC.UseVisualStyleBackColor = true;
		this.PdfC.AutoSize = true;
		this.PdfC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PdfC.Location = new System.Drawing.Point(25, 159);
		this.PdfC.Name = "PdfC";
		this.PdfC.Size = new System.Drawing.Size(171, 29);
		this.PdfC.TabIndex = 23;
		this.PdfC.Text = "Експорт в PDF";
		this.PdfC.UseVisualStyleBackColor = true;
		this.PrAc.AutoSize = true;
		this.PrAc.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PrAc.Location = new System.Drawing.Point(25, 111);
		this.PrAc.Name = "PrAc";
		this.PrAc.Size = new System.Drawing.Size(221, 29);
		this.PrAc.TabIndex = 22;
		this.PrAc.Text = "Автоматичний друк";
		this.PrAc.UseVisualStyleBackColor = true;
		this.PrXc.AutoSize = true;
		this.PrXc.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PrXc.Location = new System.Drawing.Point(25, 76);
		this.PrXc.Name = "PrXc";
		this.PrXc.Size = new System.Drawing.Size(342, 29);
		this.PrXc.TabIndex = 21;
		this.PrXc.Text = "Показувати форми друку Х звіту";
		this.PrXc.UseVisualStyleBackColor = true;
		this.PrC.AutoSize = true;
		this.PrC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.PrC.Location = new System.Drawing.Point(25, 41);
		this.PrC.Name = "PrC";
		this.PrC.Size = new System.Drawing.Size(272, 29);
		this.PrC.TabIndex = 20;
		this.PrC.Text = "Показувати форми друку";
		this.PrC.UseVisualStyleBackColor = true;
		this.FnT.Enabled = false;
		this.FnT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.FnT.Location = new System.Drawing.Point(107, 16);
		this.FnT.Name = "FnT";
		this.FnT.Size = new System.Drawing.Size(283, 30);
		this.FnT.TabIndex = 1;
		this.FnT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label2.AutoSize = true;
		this.Label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label2.Location = new System.Drawing.Point(12, 21);
		this.Label2.Name = "Label2";
		this.Label2.Size = new System.Drawing.Size(89, 25);
		this.Label2.TabIndex = 2;
		this.Label2.Text = "ПРРО №";
		this.OnC.AutoSize = true;
		this.OnC.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.OnC.Location = new System.Drawing.Point(868, 17);
		this.OnC.Name = "OnC";
		this.OnC.Size = new System.Drawing.Size(64, 29);
		this.OnC.TabIndex = 19;
		this.OnC.Text = "ON";
		this.OnC.UseVisualStyleBackColor = true;
		this.TinT.Enabled = false;
		this.TinT.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TinT.Location = new System.Drawing.Point(512, 16);
		this.TinT.Name = "TinT";
		this.TinT.Size = new System.Drawing.Size(315, 30);
		this.TinT.TabIndex = 20;
		this.TinT.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Label1.AutoSize = true;
		this.Label1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Label1.Location = new System.Drawing.Point(438, 21);
		this.Label1.Name = "Label1";
		this.Label1.Size = new System.Drawing.Size(68, 25);
		this.Label1.TabIndex = 21;
		this.Label1.Text = "TIN №";
		this.TabControlAll.Controls.Add(this.TabPage1);
		this.TabControlAll.Controls.Add(this.TabPage2);
		this.TabControlAll.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TabControlAll.Location = new System.Drawing.Point(18, 133);
		this.TabControlAll.Name = "TabControlAll";
		this.TabControlAll.SelectedIndex = 0;
		this.TabControlAll.Size = new System.Drawing.Size(976, 515);
		this.TabControlAll.TabIndex = 33;
		this.TabPage1.Controls.Add(this.GroupBox3);
		this.TabPage1.Controls.Add(this.GroupBox6);
		this.TabPage1.Controls.Add(this.GroupBox2);
		this.TabPage1.Controls.Add(this.GroupBox4);
		this.TabPage1.Font = new System.Drawing.Font("Microsoft Sans Serif", 7.8f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.TabPage1.Location = new System.Drawing.Point(4, 34);
		this.TabPage1.Name = "TabPage1";
		this.TabPage1.Padding = new System.Windows.Forms.Padding(3);
		this.TabPage1.Size = new System.Drawing.Size(968, 477);
		this.TabPage1.TabIndex = 0;
		this.TabPage1.Text = "   Основні   ";
		this.TabPage1.UseVisualStyleBackColor = true;
		this.TabPage2.Controls.Add(this.GroupBox8);
		this.TabPage2.Controls.Add(this.GroupBox7);
		this.TabPage2.Controls.Add(this.GroupBox5);
		this.TabPage2.Location = new System.Drawing.Point(4, 34);
		this.TabPage2.Name = "TabPage2";
		this.TabPage2.Padding = new System.Windows.Forms.Padding(3);
		this.TabPage2.Size = new System.Drawing.Size(968, 477);
		this.TabPage2.TabIndex = 1;
		this.TabPage2.Text = "   Інтеграція   ";
		this.TabPage2.UseVisualStyleBackColor = true;
		this.GroupBox7.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox7.Location = new System.Drawing.Point(26, 137);
		this.GroupBox7.Name = "GroupBox7";
		this.GroupBox7.Size = new System.Drawing.Size(915, 182);
		this.GroupBox7.TabIndex = 30;
		this.GroupBox7.TabStop = false;
		this.GroupBox7.Text = "Налаштування еАкциз";
		this.CBgov.AutoSize = true;
		this.CBgov.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.CBgov.Location = new System.Drawing.Point(38, 43);
		this.CBgov.Name = "CBgov";
		this.CBgov.Size = new System.Drawing.Size(757, 29);
		this.CBgov.TabIndex = 29;
		this.CBgov.Text = " Передавати додаткові дані фіскального чека до програми 'Національний чек'";
		this.CBgov.UseVisualStyleBackColor = true;
		this.GroupBox8.Controls.Add(this.CBgov);
		this.GroupBox8.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox8.Location = new System.Drawing.Point(26, 21);
		this.GroupBox8.Name = "GroupBox8";
		this.GroupBox8.Size = new System.Drawing.Size(915, 110);
		this.GroupBox8.TabIndex = 31;
		this.GroupBox8.TabStop = false;
		this.GroupBox8.Text = "Налаштування єЧек";
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1005, 659);
		base.Controls.Add(this.TabControlAll);
		base.Controls.Add(this.OnC);
		base.Controls.Add(this.Label1);
		base.Controls.Add(this.TinT);
		base.Controls.Add(this.Label2);
		base.Controls.Add(this.FnT);
		base.Controls.Add(this.GroupBox1);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormSettings";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Налаштування ПРРО";
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		this.GroupBox6.ResumeLayout(false);
		this.GroupBox6.PerformLayout();
		this.GroupBox5.ResumeLayout(false);
		this.GroupBox5.PerformLayout();
		this.GroupBox4.ResumeLayout(false);
		this.GroupBox4.PerformLayout();
		this.GroupBox3.ResumeLayout(false);
		this.GroupBox3.PerformLayout();
		this.GroupBox2.ResumeLayout(false);
		this.GroupBox2.PerformLayout();
		this.TabControlAll.ResumeLayout(false);
		this.TabPage1.ResumeLayout(false);
		this.TabPage2.ResumeLayout(false);
		this.GroupBox8.ResumeLayout(false);
		this.GroupBox8.PerformLayout();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	private void FormSettings_Load(object sender, EventArgs e)
	{
		LoadPro();
		try
		{
			ServiceController serviceController = new ServiceController("WebcheckPRROBackupService");
			if (serviceController.Status.Equals(ServiceControllerStatus.Stopped) | serviceController.Status.Equals(ServiceControllerStatus.StopPending))
			{
				LabelService.ForeColor = Color.FromArgb(200, 50, 50);
				LabelService.Text = "Служба WebСheckPRROBackup OFF";
			}
			else
			{
				LabelService.ForeColor = Color.FromArgb(50, 200, 50);
				LabelService.Text = "Служба WebСheckPRROBackup ON";
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			LabelService.ForeColor = Color.FromArgb(200, 50, 50);
			LabelService.Text = "Служба WebСheckPRROBackup OFF";
			ProjectData.ClearProjectError();
		}
		try
		{
			ServiceController serviceController2 = new ServiceController("WebСheckPRROManagementService");
			if (serviceController2.Status.Equals(ServiceControllerStatus.Stopped) | serviceController2.Status.Equals(ServiceControllerStatus.StopPending))
			{
				LabelService1.ForeColor = Color.FromArgb(200, 50, 50);
				LabelService1.Text = "Служба WebСheckPRROManagement OFF";
			}
			else
			{
				LabelService1.ForeColor = Color.FromArgb(50, 200, 50);
				LabelService1.Text = "Служба WebСheckPRROManagement ON";
			}
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			LabelService1.ForeColor = Color.FromArgb(200, 50, 50);
			LabelService1.Text = "Служба WebСheckPRROManagement OFF";
			ProjectData.ClearProjectError();
		}
		string filename = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".ini";
		IniHGB iniHGB = new IniHGB(filename);
		string text = iniHGB.GetString("Upload", "LastUpload").Trim();
		if (Operators.CompareString(text, "", TextCompare: false) != 0)
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
		GroupBox3.Enabled = All.A.FullVersion;
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
			FisB.Enabled = false;
			TesB.Enabled = true;
		}
		else
		{
			RejT.Text = "Увімкнено Тестовий режим";
			TesB.Enabled = false;
			FisB.Enabled = true;
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
		GroupBox1.Enabled = OnC.Checked;
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		if (!File.Exists(text) && !All.l.TableKsef())
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
		if (!File.Exists(text))
		{
			BackupB.Enabled = true;
			fText.Text = "вимкнено";
			lText.Text = "вимкнено";
		}
		else
		{
			BackupB.Enabled = false;
			GetInfoBackup();
		}
		TimeIni();
	}

	private void TimeIni()
	{
		string text = All.f.StringGetFn(All.A.FN, "shiftclosetime");
		string text2;
		if (Versioned.IsNumeric(text))
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
		GroupBox1.Enabled = OnC.Checked;
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
		if (Versioned.IsNumeric(DlT.Text))
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
		new FormCloseShift().ShowDialog();
		TimeIni();
	}

	private void FormSettings_Closing(object sender, CancelEventArgs e)
	{
		try
		{
			if (BackupB.Enabled && UpLoadOrder)
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
		if (Versioned.IsNumeric(IndYt.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "IndicatorY", Conversions.ToInteger(IndYt.Text));
		}
	}

	private void IndOt_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric(IndOt.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "IndicatorStepY", Conversions.ToInteger(IndOt.Text));
		}
	}

	private void SelSwrver_Click(object sender, EventArgs e)
	{
		new FormServerSelection(NewBase: false).ShowDialog();
		Server.Text = All.SF.Servers(All.f.IntegerGetFn(All.A.FN, "Acsksettings")).Name;
	}

	private void AcsC_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "UseACSKTSPserver", Math.Abs(0 - (AcsC.Checked ? 1 : 0)));
	}

	private void TesB_Click(object sender, EventArgs e)
	{
		RejT.Text = "Увімкнено Тестовий режим";
		TesB.Enabled = false;
		FisB.Enabled = true;
		All.f.IntigerWriteFN(All.A.FN, "FiscalMode", 0);
	}

	private void FisB_Click(object sender, EventArgs e)
	{
		RejT.Text = "Увімкнено ФІСКАЛЬНИЙ РЕЖИМ";
		FisB.Enabled = false;
		TesB.Enabled = true;
		All.f.IntigerWriteFN(All.A.FN, "FiscalMode", 1);
	}

	private void MinT_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric(MinT.Text))
		{
			All.f.IntigerWriteFN(All.A.FN, "OfflineMin", Conversions.ToInteger(MinT.Text));
		}
	}

	private void MaxT_TextChanged(object sender, EventArgs e)
	{
		if (Versioned.IsNumeric(MaxT.Text))
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
		BackupB.Enabled = false;
		if (!All.A.FullVersion)
		{
			Interaction.MsgBox("Ведення резервної бази доступне лише у повній версії!", MsgBoxStyle.OkOnly, "Backup");
			BackupB.Enabled = true;
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
			Interaction.MsgBox("Ведення резервної распочато!", MsgBoxStyle.OkOnly, "Backup");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		if (!File.Exists(text))
		{
			BackupB.Enabled = true;
			fText.Text = "вимкнено";
			lText.Text = "вимкнено";
		}
		else
		{
			BackupB.Enabled = false;
			GetInfoBackup();
		}
	}

	private void FormSettings_FormClosing(object sender, FormClosingEventArgs e)
	{
		string text = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db";
		if (!File.Exists(text) && !All.l.TableKsef())
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
	}

	private void CBgov_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "useecheckmegovua", Math.Abs(0 - (CBgov.Checked ? 1 : 0)));
	}
}
