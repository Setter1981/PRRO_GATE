using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Printing;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using System.Xml;
using Gma.QrCodeNet.Encoding;
using Gma.QrCodeNet.Encoding.Windows.Forms;
using Gma.QrCodeNet.Encoding.Windows.Render;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using iTextSharp.text;
using iTextSharp.text.pdf;

namespace WebCheck;

[DesignerGenerated]
internal class FormPrint : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("Druk")]
	private Button _Druk;

	[CompilerGenerated]
	[AccessedThroughProperty("Rb2")]
	private RadioButton _Rb2;

	[CompilerGenerated]
	[AccessedThroughProperty("Rb1")]
	private RadioButton _Rb1;

	[CompilerGenerated]
	[AccessedThroughProperty("PrintDocument1")]
	private PrintDocument _PrintDocument1;

	[CompilerGenerated]
	[AccessedThroughProperty("ДрукToolStripMenuItem")]
	private ToolStripMenuItem _ДрукToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("НалаштуванняДрукуToolStripMenuItem")]
	private ToolStripMenuItem _НалаштуванняДрукуToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЗакритиToolStripMenuItem")]
	private ToolStripMenuItem _ЗакритиToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ВибірПринтераToolStripMenuItem")]
	private ToolStripMenuItem _ВибірПринтераToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ОстаннійЧекToolStripMenuItem")]
	private ToolStripMenuItem _ОстаннійЧекToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ОстаннійZЗвітToolStripMenuItem")]
	private ToolStripMenuItem _ОстаннійZЗвітToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ЕкспортВToolStripMenuItem")]
	private ToolStripMenuItem _ЕкспортВToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("QrCode")]
	private QrCodeImgControl _QrCode;

	[CompilerGenerated]
	[AccessedThroughProperty("LinkCopy")]
	private Button _LinkCopy;

	[CompilerGenerated]
	[AccessedThroughProperty("EndB")]
	private Button _EndB;

	[CompilerGenerated]
	[AccessedThroughProperty("ВсіЗміниToolStripMenuItem")]
	private ToolStripMenuItem _ВсіЗміниToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("SmsB")]
	private Button _SmsB;

	[CompilerGenerated]
	[AccessedThroughProperty("ЛінкВБуферОбмінуToolStripMenuItem")]
	private ToolStripMenuItem _ЛінкВБуферОбмінуToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("CheckEco")]
	private CheckBox _CheckEco;

	public bool CloseForm;

	public string nPrint;

	private int Dlstr;

	private string[] StrCheck;

	private string[] StrCheckR;

	private string[] StrCheckN;

	private bool Zapolnili;

	private string xZvit;

	private string LincWWW;

	private string DataWWW;

	private string TimeWWW;

	private int TypWWW;

	private string MacPr;

	private string DataTimePr;

	private string FiChPr;

	private string SumPr;

	private string FnPr;

	private System.Drawing.Image PrintLogo;

	private string EndDateKeyPay;

	[field: AccessedThroughProperty("Tb")]
	internal virtual TextBox Tb
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button Druk
	{
		[CompilerGenerated]
		get
		{
			return _Druk;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Druk_Click;
			Button druk = _Druk;
			if (druk != null)
			{
				druk.Click -= value2;
			}
			_Druk = value;
			druk = _Druk;
			if (druk != null)
			{
				druk.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("sPrint")]
	internal virtual GroupBox sPrint
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual RadioButton Rb2
	{
		[CompilerGenerated]
		get
		{
			return _Rb2;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Rb2_CheckedChanged;
			RadioButton rb = _Rb2;
			if (rb != null)
			{
				rb.CheckedChanged -= value2;
			}
			_Rb2 = value;
			rb = _Rb2;
			if (rb != null)
			{
				rb.CheckedChanged += value2;
			}
		}
	}

	internal virtual RadioButton Rb1
	{
		[CompilerGenerated]
		get
		{
			return _Rb1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = Rb1_CheckedChanged;
			RadioButton rb = _Rb1;
			if (rb != null)
			{
				rb.CheckedChanged -= value2;
			}
			_Rb1 = value;
			rb = _Rb1;
			if (rb != null)
			{
				rb.CheckedChanged += value2;
			}
		}
	}

	internal virtual PrintDocument PrintDocument1
	{
		[CompilerGenerated]
		get
		{
			return _PrintDocument1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			PrintPageEventHandler value2 = PrintDocument1_PrintPage;
			PrintDocument printDocument = _PrintDocument1;
			if (printDocument != null)
			{
				printDocument.PrintPage -= value2;
			}
			_PrintDocument1 = value;
			printDocument = _PrintDocument1;
			if (printDocument != null)
			{
				printDocument.PrintPage += value2;
			}
		}
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ДрукToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ДрукToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ДрукToolStripMenuItem_Click;
			ToolStripMenuItem друкToolStripMenuItem = _ДрукToolStripMenuItem;
			if (друкToolStripMenuItem != null)
			{
				друкToolStripMenuItem.Click -= value2;
			}
			_ДрукToolStripMenuItem = value;
			друкToolStripMenuItem = _ДрукToolStripMenuItem;
			if (друкToolStripMenuItem != null)
			{
				друкToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem НалаштуванняДрукуToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _НалаштуванняДрукуToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = НалаштуванняДрукуToolStripMenuItem_Click;
			ToolStripMenuItem налаштуванняДрукуToolStripMenuItem = _НалаштуванняДрукуToolStripMenuItem;
			if (налаштуванняДрукуToolStripMenuItem != null)
			{
				налаштуванняДрукуToolStripMenuItem.Click -= value2;
			}
			_НалаштуванняДрукуToolStripMenuItem = value;
			налаштуванняДрукуToolStripMenuItem = _НалаштуванняДрукуToolStripMenuItem;
			if (налаштуванняДрукуToolStripMenuItem != null)
			{
				налаштуванняДрукуToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem ЗакритиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЗакритиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЗакритиToolStripMenuItem_Click;
			ToolStripMenuItem закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click -= value2;
			}
			_ЗакритиToolStripMenuItem = value;
			закритиToolStripMenuItem = _ЗакритиToolStripMenuItem;
			if (закритиToolStripMenuItem != null)
			{
				закритиToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("PrintDialog1")]
	internal virtual PrintDialog PrintDialog1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("PrintPreviewDialog1")]
	internal virtual PrintPreviewDialog PrintPreviewDialog1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ВибірПринтераToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВибірПринтераToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ВибірПринтераToolStripMenuItem_Click;
			ToolStripMenuItem вибірПринтераToolStripMenuItem = _ВибірПринтераToolStripMenuItem;
			if (вибірПринтераToolStripMenuItem != null)
			{
				вибірПринтераToolStripMenuItem.Click -= value2;
			}
			_ВибірПринтераToolStripMenuItem = value;
			вибірПринтераToolStripMenuItem = _ВибірПринтераToolStripMenuItem;
			if (вибірПринтераToolStripMenuItem != null)
			{
				вибірПринтераToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("GroupBox1")]
	internal virtual GroupBox GroupBox1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ToolStripMenuItem2")]
	internal virtual ToolStripSeparator ToolStripMenuItem2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TB1")]
	internal virtual TextBox TB1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("ПоказатиToolStripMenuItem")]
	internal virtual ToolStripMenuItem ПоказатиToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ОстаннійЧекToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОстаннійЧекToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ОстаннійЧекToolStripMenuItem_Click;
			ToolStripMenuItem останнійЧекToolStripMenuItem = _ОстаннійЧекToolStripMenuItem;
			if (останнійЧекToolStripMenuItem != null)
			{
				останнійЧекToolStripMenuItem.Click -= value2;
			}
			_ОстаннійЧекToolStripMenuItem = value;
			останнійЧекToolStripMenuItem = _ОстаннійЧекToolStripMenuItem;
			if (останнійЧекToolStripMenuItem != null)
			{
				останнійЧекToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem ОстаннійZЗвітToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОстаннійZЗвітToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ОстаннійZЗвітToolStripMenuItem_Click;
			ToolStripMenuItem останнійZЗвітToolStripMenuItem = _ОстаннійZЗвітToolStripMenuItem;
			if (останнійZЗвітToolStripMenuItem != null)
			{
				останнійZЗвітToolStripMenuItem.Click -= value2;
			}
			_ОстаннійZЗвітToolStripMenuItem = value;
			останнійZЗвітToolStripMenuItem = _ОстаннійZЗвітToolStripMenuItem;
			if (останнійZЗвітToolStripMenuItem != null)
			{
				останнійZЗвітToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ЕкспортВToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЕкспортВToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЕкспортВToolStripMenuItem_Click;
			ToolStripMenuItem експортВToolStripMenuItem = _ЕкспортВToolStripMenuItem;
			if (експортВToolStripMenuItem != null)
			{
				експортВToolStripMenuItem.Click -= value2;
			}
			_ЕкспортВToolStripMenuItem = value;
			експортВToolStripMenuItem = _ЕкспортВToolStripMenuItem;
			if (експортВToolStripMenuItem != null)
			{
				експортВToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("TB2")]
	internal virtual TextBox TB2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual QrCodeImgControl QrCode
	{
		[CompilerGenerated]
		get
		{
			return _QrCode;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = QrCode_DoubleClick;
			EventHandler value3 = QrCode_Click;
			QrCodeImgControl qrCode = _QrCode;
			if (qrCode != null)
			{
				qrCode.DoubleClick -= value2;
				qrCode.Click -= value3;
			}
			_QrCode = value;
			qrCode = _QrCode;
			if (qrCode != null)
			{
				qrCode.DoubleClick += value2;
				qrCode.Click += value3;
			}
		}
	}

	internal virtual Button LinkCopy
	{
		[CompilerGenerated]
		get
		{
			return _LinkCopy;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = LinkCopy_Click;
			Button linkCopy = _LinkCopy;
			if (linkCopy != null)
			{
				linkCopy.Click -= value2;
			}
			_LinkCopy = value;
			linkCopy = _LinkCopy;
			if (linkCopy != null)
			{
				linkCopy.Click += value2;
			}
		}
	}

	internal virtual Button EndB
	{
		[CompilerGenerated]
		get
		{
			return _EndB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = EndB_Click;
			Button endB = _EndB;
			if (endB != null)
			{
				endB.Click -= value2;
			}
			_EndB = value;
			endB = _EndB;
			if (endB != null)
			{
				endB.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem3")]
	internal virtual ToolStripSeparator ToolStripMenuItem3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ВсіЗміниToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ВсіЗміниToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ВсіЗміниToolStripMenuItem_Click;
			ToolStripMenuItem всіЗміниToolStripMenuItem = _ВсіЗміниToolStripMenuItem;
			if (всіЗміниToolStripMenuItem != null)
			{
				всіЗміниToolStripMenuItem.Click -= value2;
			}
			_ВсіЗміниToolStripMenuItem = value;
			всіЗміниToolStripMenuItem = _ВсіЗміниToolStripMenuItem;
			if (всіЗміниToolStripMenuItem != null)
			{
				всіЗміниToolStripMenuItem.Click += value2;
			}
		}
	}

	internal virtual Button SmsB
	{
		[CompilerGenerated]
		get
		{
			return _SmsB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = SmsB_Click;
			Button smsB = _SmsB;
			if (smsB != null)
			{
				smsB.Click -= value2;
			}
			_SmsB = value;
			smsB = _SmsB;
			if (smsB != null)
			{
				smsB.Click += value2;
			}
		}
	}

	internal virtual ToolStripMenuItem ЛінкВБуферОбмінуToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ЛінкВБуферОбмінуToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = ЛінкВБуферОбмінуToolStripMenuItem_Click;
			ToolStripMenuItem лінкВБуферОбмінуToolStripMenuItem = _ЛінкВБуферОбмінуToolStripMenuItem;
			if (лінкВБуферОбмінуToolStripMenuItem != null)
			{
				лінкВБуферОбмінуToolStripMenuItem.Click -= value2;
			}
			_ЛінкВБуферОбмінуToolStripMenuItem = value;
			лінкВБуферОбмінуToolStripMenuItem = _ЛінкВБуферОбмінуToolStripMenuItem;
			if (лінкВБуферОбмінуToolStripMenuItem != null)
			{
				лінкВБуферОбмінуToolStripMenuItem.Click += value2;
			}
		}
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox CheckEco
	{
		[CompilerGenerated]
		get
		{
			return _CheckEco;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler value2 = CheckEco_CheckedChanged;
			CheckBox checkEco = _CheckEco;
			if (checkEco != null)
			{
				checkEco.CheckedChanged -= value2;
			}
			_CheckEco = value;
			checkEco = _CheckEco;
			if (checkEco != null)
			{
				checkEco.CheckedChanged += value2;
			}
		}
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
		this.components = new System.ComponentModel.Container();
		System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(WebCheck.FormPrint));
		this.Tb = new System.Windows.Forms.TextBox();
		this.Druk = new System.Windows.Forms.Button();
		this.sPrint = new System.Windows.Forms.GroupBox();
		this.CheckEco = new System.Windows.Forms.CheckBox();
		this.Rb2 = new System.Windows.Forms.RadioButton();
		this.Rb1 = new System.Windows.Forms.RadioButton();
		this.PrintDocument1 = new System.Drawing.Printing.PrintDocument();
		this.MenuStrip1 = new System.Windows.Forms.MenuStrip();
		this.МенюToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ДрукToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.НалаштуванняДрукуToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ВибірПринтераToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem1 = new System.Windows.Forms.ToolStripSeparator();
		this.ЛінкВБуферОбмінуToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ЕкспортВToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem2 = new System.Windows.Forms.ToolStripSeparator();
		this.ЗакритиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ПоказатиToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ОстаннійЧекToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ОстаннійZЗвітToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.ToolStripMenuItem3 = new System.Windows.Forms.ToolStripSeparator();
		this.ВсіЗміниToolStripMenuItem = new System.Windows.Forms.ToolStripMenuItem();
		this.PrintDialog1 = new System.Windows.Forms.PrintDialog();
		this.PrintPreviewDialog1 = new System.Windows.Forms.PrintPreviewDialog();
		this.GroupBox1 = new System.Windows.Forms.GroupBox();
		this.TB2 = new System.Windows.Forms.TextBox();
		this.TB1 = new System.Windows.Forms.TextBox();
		this.QrCode = new Gma.QrCodeNet.Encoding.Windows.Forms.QrCodeImgControl();
		this.LinkCopy = new System.Windows.Forms.Button();
		this.EndB = new System.Windows.Forms.Button();
		this.SmsB = new System.Windows.Forms.Button();
		this.ToolTip1 = new System.Windows.Forms.ToolTip(this.components);
		this.sPrint.SuspendLayout();
		this.MenuStrip1.SuspendLayout();
		this.GroupBox1.SuspendLayout();
		((System.ComponentModel.ISupportInitialize)this.QrCode).BeginInit();
		base.SuspendLayout();
		this.Tb.BackColor = System.Drawing.Color.White;
		this.Tb.BorderStyle = System.Windows.Forms.BorderStyle.None;
		this.Tb.Font = new System.Drawing.Font("Consolas", 10.2f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Tb.Location = new System.Drawing.Point(15, 41);
		this.Tb.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Tb.Multiline = true;
		this.Tb.Name = "Tb";
		this.Tb.ReadOnly = true;
		this.Tb.ScrollBars = System.Windows.Forms.ScrollBars.Vertical;
		this.Tb.Size = new System.Drawing.Size(604, 569);
		this.Tb.TabIndex = 1;
		this.Tb.TabStop = false;
		this.Tb.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.Druk.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.Druk.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.Druk.Location = new System.Drawing.Point(902, 319);
		this.Druk.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Druk.Name = "Druk";
		this.Druk.Size = new System.Drawing.Size(133, 39);
		this.Druk.TabIndex = 0;
		this.Druk.Text = "ДРУК";
		this.ToolTip1.SetToolTip(this.Druk, "Друк чека ");
		this.Druk.UseVisualStyleBackColor = true;
		this.sPrint.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.sPrint.Controls.Add(this.CheckEco);
		this.sPrint.Controls.Add(this.Rb2);
		this.sPrint.Controls.Add(this.Rb1);
		this.sPrint.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.sPrint.Location = new System.Drawing.Point(786, 175);
		this.sPrint.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.sPrint.Name = "sPrint";
		this.sPrint.Padding = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.sPrint.Size = new System.Drawing.Size(249, 128);
		this.sPrint.TabIndex = 2;
		this.sPrint.TabStop = false;
		this.sPrint.Text = "Ширина стрічки:";
		this.CheckEco.AutoSize = true;
		this.CheckEco.Location = new System.Drawing.Point(36, 83);
		this.CheckEco.Name = "CheckEco";
		this.CheckEco.Size = new System.Drawing.Size(187, 29);
		this.CheckEco.TabIndex = 2;
		this.CheckEco.Text = "Економний друк";
		this.CheckEco.UseVisualStyleBackColor = true;
		this.Rb2.AutoSize = true;
		this.Rb2.Location = new System.Drawing.Point(139, 40);
		this.Rb2.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Rb2.Name = "Rb2";
		this.Rb2.Size = new System.Drawing.Size(94, 29);
		this.Rb2.TabIndex = 1;
		this.Rb2.TabStop = true;
		this.Rb2.Text = "80 мм";
		this.ToolTip1.SetToolTip(this.Rb2, "Ширина стрічки  80 мм");
		this.Rb2.UseVisualStyleBackColor = true;
		this.Rb1.AutoSize = true;
		this.Rb1.Location = new System.Drawing.Point(19, 40);
		this.Rb1.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.Rb1.Name = "Rb1";
		this.Rb1.Size = new System.Drawing.Size(94, 29);
		this.Rb1.TabIndex = 0;
		this.Rb1.TabStop = true;
		this.Rb1.Text = "57 мм";
		this.ToolTip1.SetToolTip(this.Rb1, "Ширина стрічки  57 мм");
		this.Rb1.UseVisualStyleBackColor = true;
		this.MenuStrip1.ImageScalingSize = new System.Drawing.Size(20, 20);
		this.MenuStrip1.Items.AddRange(new System.Windows.Forms.ToolStripItem[2] { this.МенюToolStripMenuItem, this.ПоказатиToolStripMenuItem });
		this.MenuStrip1.Location = new System.Drawing.Point(0, 0);
		this.MenuStrip1.Name = "MenuStrip1";
		this.MenuStrip1.Padding = new System.Windows.Forms.Padding(5, 2, 0, 2);
		this.MenuStrip1.Size = new System.Drawing.Size(1053, 28);
		this.MenuStrip1.TabIndex = 3;
		this.MenuStrip1.Text = "MenuStrip1";
		this.МенюToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[8] { this.ДрукToolStripMenuItem, this.НалаштуванняДрукуToolStripMenuItem, this.ВибірПринтераToolStripMenuItem, this.ToolStripMenuItem1, this.ЛінкВБуферОбмінуToolStripMenuItem, this.ЕкспортВToolStripMenuItem, this.ToolStripMenuItem2, this.ЗакритиToolStripMenuItem });
		this.МенюToolStripMenuItem.Name = "МенюToolStripMenuItem";
		this.МенюToolStripMenuItem.Size = new System.Drawing.Size(65, 24);
		this.МенюToolStripMenuItem.Text = "Меню";
		this.ДрукToolStripMenuItem.Name = "ДрукToolStripMenuItem";
		this.ДрукToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.ДрукToolStripMenuItem.Text = "Друк";
		this.НалаштуванняДрукуToolStripMenuItem.Name = "НалаштуванняДрукуToolStripMenuItem";
		this.НалаштуванняДрукуToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.НалаштуванняДрукуToolStripMenuItem.Text = "Попередній перегляд...";
		this.ВибірПринтераToolStripMenuItem.Name = "ВибірПринтераToolStripMenuItem";
		this.ВибірПринтераToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.ВибірПринтераToolStripMenuItem.Text = "Вибір принтера ...";
		this.ToolStripMenuItem1.Name = "ToolStripMenuItem1";
		this.ToolStripMenuItem1.Size = new System.Drawing.Size(250, 6);
		this.ЛінкВБуферОбмінуToolStripMenuItem.Name = "ЛінкВБуферОбмінуToolStripMenuItem";
		this.ЛінкВБуферОбмінуToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.ЛінкВБуферОбмінуToolStripMenuItem.Text = "Лінк в буфер обміну";
		this.ЕкспортВToolStripMenuItem.Name = "ЕкспортВToolStripMenuItem";
		this.ЕкспортВToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.ЕкспортВToolStripMenuItem.Text = "Експорт в PDF";
		this.ToolStripMenuItem2.Name = "ToolStripMenuItem2";
		this.ToolStripMenuItem2.Size = new System.Drawing.Size(250, 6);
		this.ЗакритиToolStripMenuItem.Name = "ЗакритиToolStripMenuItem";
		this.ЗакритиToolStripMenuItem.Size = new System.Drawing.Size(253, 26);
		this.ЗакритиToolStripMenuItem.Text = "Закрити";
		this.ПоказатиToolStripMenuItem.DropDownItems.AddRange(new System.Windows.Forms.ToolStripItem[4] { this.ОстаннійЧекToolStripMenuItem, this.ОстаннійZЗвітToolStripMenuItem, this.ToolStripMenuItem3, this.ВсіЗміниToolStripMenuItem });
		this.ПоказатиToolStripMenuItem.Name = "ПоказатиToolStripMenuItem";
		this.ПоказатиToolStripMenuItem.Size = new System.Drawing.Size(88, 24);
		this.ПоказатиToolStripMenuItem.Text = "Показати";
		this.ОстаннійЧекToolStripMenuItem.Name = "ОстаннійЧекToolStripMenuItem";
		this.ОстаннійЧекToolStripMenuItem.Size = new System.Drawing.Size(197, 26);
		this.ОстаннійЧекToolStripMenuItem.Text = "Останній чек";
		this.ОстаннійZЗвітToolStripMenuItem.Name = "ОстаннійZЗвітToolStripMenuItem";
		this.ОстаннійZЗвітToolStripMenuItem.Size = new System.Drawing.Size(197, 26);
		this.ОстаннійZЗвітToolStripMenuItem.Text = "Останній Z звіт";
		this.ToolStripMenuItem3.Name = "ToolStripMenuItem3";
		this.ToolStripMenuItem3.Size = new System.Drawing.Size(194, 6);
		this.ВсіЗміниToolStripMenuItem.Name = "ВсіЗміниToolStripMenuItem";
		this.ВсіЗміниToolStripMenuItem.Size = new System.Drawing.Size(197, 26);
		this.ВсіЗміниToolStripMenuItem.Text = "Всі Зміни...";
		this.PrintDialog1.UseEXDialog = true;
		this.PrintPreviewDialog1.AutoScrollMargin = new System.Drawing.Size(0, 0);
		this.PrintPreviewDialog1.AutoScrollMinSize = new System.Drawing.Size(0, 0);
		this.PrintPreviewDialog1.ClientSize = new System.Drawing.Size(400, 300);
		this.PrintPreviewDialog1.Enabled = true;
		this.PrintPreviewDialog1.Icon = (System.Drawing.Icon)resources.GetObject("PrintPreviewDialog1.Icon");
		this.PrintPreviewDialog1.Name = "PrintPreviewDialog1";
		this.PrintPreviewDialog1.Visible = false;
		this.GroupBox1.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.GroupBox1.Controls.Add(this.TB2);
		this.GroupBox1.Controls.Add(this.TB1);
		this.GroupBox1.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.GroupBox1.Location = new System.Drawing.Point(786, 41);
		this.GroupBox1.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox1.Name = "GroupBox1";
		this.GroupBox1.Padding = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.GroupBox1.Size = new System.Drawing.Size(249, 124);
		this.GroupBox1.TabIndex = 4;
		this.GroupBox1.TabStop = false;
		this.GroupBox1.Text = "Чек №";
		this.TB2.Enabled = false;
		this.TB2.Location = new System.Drawing.Point(16, 32);
		this.TB2.Name = "TB2";
		this.TB2.Size = new System.Drawing.Size(217, 30);
		this.TB2.TabIndex = 1;
		this.TB2.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.TB1.Enabled = false;
		this.TB1.Location = new System.Drawing.Point(16, 75);
		this.TB1.Name = "TB1";
		this.TB1.Size = new System.Drawing.Size(217, 30);
		this.TB1.TabIndex = 0;
		this.TB1.TextAlign = System.Windows.Forms.HorizontalAlignment.Center;
		this.QrCode.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.QrCode.ErrorCorrectLevel = Gma.QrCodeNet.Encoding.ErrorCorrectionLevel.M;
		this.QrCode.Image = (System.Drawing.Image)resources.GetObject("QrCode.Image");
		this.QrCode.Location = new System.Drawing.Point(820, 370);
		this.QrCode.Name = "QrCode";
		this.QrCode.QuietZoneModule = Gma.QrCodeNet.Encoding.Windows.Render.QuietZoneModules.Two;
		this.QrCode.Size = new System.Drawing.Size(190, 190);
		this.QrCode.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
		this.QrCode.TabIndex = 5;
		this.QrCode.TabStop = false;
		this.QrCode.Text = "QrCodeImgControl1";
		this.ToolTip1.SetToolTip(this.QrCode, "QR код чека");
		this.LinkCopy.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.LinkCopy.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.LinkCopy.Location = new System.Drawing.Point(665, 572);
		this.LinkCopy.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.LinkCopy.Name = "LinkCopy";
		this.LinkCopy.Size = new System.Drawing.Size(103, 38);
		this.LinkCopy.TabIndex = 10;
		this.LinkCopy.Text = "Copy";
		this.LinkCopy.UseVisualStyleBackColor = true;
		this.LinkCopy.Visible = false;
		this.EndB.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.EndB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.EndB.Location = new System.Drawing.Point(786, 319);
		this.EndB.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.EndB.Name = "EndB";
		this.EndB.Size = new System.Drawing.Size(110, 39);
		this.EndB.TabIndex = 11;
		this.EndB.Text = "Закрити";
		this.ToolTip1.SetToolTip(this.EndB, "Закрити вікно");
		this.EndB.UseVisualStyleBackColor = true;
		this.SmsB.Anchor = System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right;
		this.SmsB.Font = new System.Drawing.Font("Microsoft Sans Serif", 12f, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, 204);
		this.SmsB.Location = new System.Drawing.Point(786, 572);
		this.SmsB.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		this.SmsB.Name = "SmsB";
		this.SmsB.Size = new System.Drawing.Size(249, 38);
		this.SmsB.TabIndex = 12;
		this.SmsB.Text = "VIBER/SMS...";
		this.ToolTip1.SetToolTip(this.SmsB, "Надіслати чек покупцю на Вайбер або СМС ");
		this.SmsB.UseVisualStyleBackColor = true;
		base.AutoScaleDimensions = new System.Drawing.SizeF(8f, 16f);
		base.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
		base.ClientSize = new System.Drawing.Size(1053, 621);
		base.Controls.Add(this.SmsB);
		base.Controls.Add(this.EndB);
		base.Controls.Add(this.LinkCopy);
		base.Controls.Add(this.QrCode);
		base.Controls.Add(this.GroupBox1);
		base.Controls.Add(this.sPrint);
		base.Controls.Add(this.Druk);
		base.Controls.Add(this.Tb);
		base.Controls.Add(this.MenuStrip1);
		base.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
		base.Icon = (System.Drawing.Icon)resources.GetObject("$this.Icon");
		base.MainMenuStrip = this.MenuStrip1;
		base.Margin = new System.Windows.Forms.Padding(3, 2, 3, 2);
		base.MaximizeBox = false;
		base.MinimizeBox = false;
		base.Name = "FormPrint";
		base.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
		this.Text = "Друк чека";
		base.TopMost = true;
		this.sPrint.ResumeLayout(false);
		this.sPrint.PerformLayout();
		this.MenuStrip1.ResumeLayout(false);
		this.MenuStrip1.PerformLayout();
		this.GroupBox1.ResumeLayout(false);
		this.GroupBox1.PerformLayout();
		((System.ComponentModel.ISupportInitialize)this.QrCode).EndInit();
		base.ResumeLayout(false);
		base.PerformLayout();
	}

	public FormPrint(string strN, string XMLx = "", int typXML = 0)
	{
		base.Load += FormPrint_Load;
		CloseForm = false;
		Dlstr = 29;
		StrCheck = new string[3];
		StrCheckR = new string[3];
		StrCheckN = new string[1];
		Zapolnili = false;
		xZvit = "";
		LincWWW = "https://cabinet.tax.gov.ua/cashregs/check?id=";
		DataWWW = "";
		TimeWWW = "";
		InitializeComponent();
		if (XMLx.Length > 18 || typXML > 0)
		{
			switch (typXML)
			{
			case 0:
				xZvit = XMLx;
				nPrint = "xZvit";
				break;
			case 1:
				xZvit = XMLx;
				nPrint = "pZvit";
				break;
			}
			return;
		}
		TypErrStr parametrToString = All.d.GetParametrToString(strN, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strN, "taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strN, "Taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strN, "TAXNUM", "InputParameters/Parameters", RegUpLow: true);
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", TextCompare: false) == 0)
		{
			parametrToString = All.d.GetParametrToString(strN, "id");
		}
		nPrint = parametrToString.ReturnStr;
	}

	private void FormPrint_Load(object sender, EventArgs e)
	{
		int integer = All.f.GetInteger("Global", "FormPrintY", 0);
		if (integer > 0)
		{
			int integer2 = All.f.GetInteger("Global", "FormPrintX", 0);
			base.Top = integer;
			base.Left = integer2;
		}
		CloseForm = false;
		if (!All.A.FullVersion)
		{
			All.A.ecoPrint = false;
			All.f.StringWriteFN(All.A.FN, "EcoPrt", "0");
		}
		CheckEco.Checked = All.A.ecoPrint;
		if (Operators.CompareString(All.A.PrinterName, "", TextCompare: false) != 0)
		{
			PrintDialog1.PrinterSettings.PrinterName = All.A.PrinterName;
		}
		if (All.A.FullVersion & All.A.AutomatPrintCheck)
		{
			base.Top = -9000;
		}
		LoadImg();
		Application.DoEvents();
		base.Width = 850;
		Tb.Width = 453;
		base.AcceptButton = Druk;
		base.CancelButton = EndB;
		if (All.A.FullVersion & All.A.AutomatPrintCheck)
		{
			Show();
		}
		switch (All.f.IntegerGetFn(All.A.FN, "PrinterWidth"))
		{
		case 57:
			Rb1.Checked = true;
			break;
		case 80:
			Rb2.Checked = true;
			break;
		default:
			Rb1.Checked = true;
			break;
		}
	}

	private void LoadImg()
	{
		try
		{
			string text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".bmp";
			if (File.Exists(text))
			{
				PrintLogo = System.Drawing.Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".jpg";
			if (File.Exists(text))
			{
				PrintLogo = System.Drawing.Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".png";
			if (File.Exists(text))
			{
				PrintLogo = System.Drawing.Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\logo.png";
			if (File.Exists(text))
			{
				PrintLogo = System.Drawing.Image.FromFile(text);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void ResCheck()
	{
		if (!Zapolnili)
		{
			MacPr = "";
			DataTimePr = "";
			FiChPr = "";
			SumPr = "";
			FnPr = "";
			ZApolnit();
		}
		LincWWW = "https://cabinet.tax.gov.ua/cashregs/check?id=" + TB2.Text + "&date=" + DataWWW + "&time=" + TimeWWW + "&fn=" + FnPr + "&sm=" + SumPr;
		if (Operators.CompareString(MacPr, null, TextCompare: false) != 0 && MacPr.Length > 1)
		{
			string text = "&mac=" + MacPr;
			LincWWW += text;
		}
		QrCode.Text = LincWWW;
		Perenos();
		Tb.Text = "";
		checked
		{
			int num = StrCheckN.Count() - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(StrCheckN[i], null, TextCompare: false) == 0)
				{
					StrCheckN[i] = "";
				}
				if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", TextCompare: false) != 0)
				{
					TextBox tb;
					(tb = Tb).Text = tb.Text + StrCheckN[i] + "\r\n";
				}
			}
		}
	}

	private void ZApolnit()
	{
		Zapolnili = true;
		TypPrintChecks typPrintChecks = ((Operators.CompareString(nPrint.ToLower(), "z", TextCompare: false) == 0) ? All.Rf.CheckXMLz() : (Versioned.IsNumeric(nPrint) ? All.Rf.CheckXMLNumber(nPrint, SearchID: true) : ((Operators.CompareString(nPrint.Trim(), "", TextCompare: false) == 0) ? All.Rf.CheckXMLNumber(nPrint) : checked(((Operators.CompareString(Conversions.ToString(nPrint[nPrint.Length - 1]), "Z", TextCompare: false) == 0) & (nPrint.Length < 9)) ? All.Rf.CheckXMLNumber(nPrint.ToUpper()) : ((!((Operators.CompareString(Conversions.ToString(nPrint[nPrint.Length - 1]), "z", TextCompare: false) == 0) & (nPrint.Length < 9))) ? All.Rf.CheckXMLNumberTax(nPrint) : All.Rf.CheckXMLNumber(nPrint.ToUpper()))))));
		int num;
		if (Operators.CompareString(nPrint, "xZvit", TextCompare: false) == 0)
		{
			num = 4;
			TB1.Text = "X ЗВІТ";
			TB2.Text = "ЗМІНА № " + All.l.ReturnOpenShift().ReturnStr;
			Application.DoEvents();
		}
		else if (Operators.CompareString(nPrint, "pZvit", TextCompare: false) == 0)
		{
			num = 5;
			TB1.Text = "ЗВЕДЕНИЙ ЗВІТ";
			TB2.Text = "СЛУЖБОВИЙ ЧЕК";
			Application.DoEvents();
		}
		else
		{
			TB1.Text = typPrintChecks.ReturnStrN;
			TB2.Text = typPrintChecks.ReturnStrTaxN;
			Application.DoEvents();
			num = TypChekcs(typPrintChecks.ReturnStr);
			if (num == -8)
			{
				num = 8;
			}
		}
		StrCheck = new string[1];
		StrCheckR = new string[1];
		StrCheck[0] = "";
		StrCheckR[0] = "";
		if (num > 2 || num < 0)
		{
			EndPayKey();
		}
		ref string[] strCheck = ref StrCheck;
		checked
		{
			strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR = ref StrCheckR;
			strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = All.A.OrgName;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = All.A.PointName;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = All.A.PointAddr;
			StrCheckR[StrCheck.Count() - 1] = "";
			if (All.A.INN.Trim().Length > 1)
			{
				ref string[] strCheck4 = ref StrCheck;
				strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR4 = ref StrCheckR;
				strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПН " + All.A.INN;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			if (All.A.TIN.Trim().Length > 1)
			{
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ІД " + All.A.TIN;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			string onOf = "офлайн";
			if (Operators.CompareString(typPrintChecks.ReturnOffline, "0", TextCompare: false) == 0)
			{
				onOf = "онлайн";
			}
			SmsB.Enabled = false;
			TypWWW = num;
			switch (num)
			{
			case 0:
				SmsB.Enabled = true;
				CloseForm = true;
				XMLtoDim(typPrintChecks.ReturnStr, vosvrat: false, onOf, typPrintChecks.ReturnMac);
				break;
			case 1:
				SmsB.Enabled = true;
				CloseForm = true;
				XMLtoDim(typPrintChecks.ReturnStr, vosvrat: true, onOf, typPrintChecks.ReturnMac);
				break;
			case 2:
				CloseForm = false;
				XMLtoDimS(typPrintChecks.ReturnStr, onOf);
				break;
			case 3:
				CloseForm = false;
				XMLtoDimZ(typPrintChecks.ReturnStr, onOf);
				break;
			case 4:
				CloseForm = false;
				XMLtoDimX(xZvit);
				break;
			case 5:
				CloseForm = false;
				XMLtoDimPeriod(xZvit);
				break;
			case 8:
				SmsB.Enabled = true;
				CloseForm = true;
				XMLtoDimEPZ(typPrintChecks.ReturnStr, onOf, typPrintChecks.ReturnMac);
				break;
			default:
				CloseForm = false;
				XMLtoAll(typPrintChecks.ReturnStr, onOf);
				break;
			}
		}
	}

	private void EndPayKey()
	{
		if (All.A.FullVersion && DateDateInfa(All.A.Fullend))
		{
			EndPay();
		}
		if (KeyShiftTime())
		{
			EndKey();
		}
	}

	internal bool KeyShiftTime()
	{
		string returnStr = All.l.INNoperatorInShift(All.l.MaxID("SHIFTS").ReturnStr).ReturnStr;
		bool result;
		try
		{
			DateTime now = DateTime.Now;
			DateTime date = Conversions.ToDate(EndDateKeyPay = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\dat.ini").GetString(returnStr, "EndKey").Trim());
			result = checked((int)DateAndTime.DateDiff(DateInterval.Day, now, date)) < 15;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private bool DateDateInfa(string DateEnd)
	{
		EndDateKeyPay = All.A.Fullend;
		bool result;
		try
		{
			result = ((Math.Abs(DateAndTime.DateDiff(DateInterval.Day, DateTime.Now, Conversions.ToDate(DateEnd))) < 15) ? true : false);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private void EndPay()
	{
		ref string[] strCheck = ref StrCheck;
		checked
		{
			strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR = ref StrCheckR;
			strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\dealerinfo.ini");
			string text = iniHGB.GetString("Global", "1");
			string text2;
			string text3;
			string text4;
			string text5;
			string text6;
			string text7;
			if (Operators.CompareString(text, "", TextCompare: false) != 0)
			{
				text2 = "!!! ШАНОВНИЙ КЛІЄНТ !!!";
				text3 = "!!! УВАГА !!!";
				text4 = "НАГАДУЄМО ВАМ, ЩО " + EndDateKeyPay;
				text5 = "ТЕРМІН ДІЇ ЛІЦЕНЗІЇЇ НА ПРРО ЗАКІНЧУЄТЬСЯ.";
				text6 = iniHGB.GetString("Global", "2");
				text7 = iniHGB.GetString("Global", "3");
			}
			else
			{
				text2 = "!!! ШАНОВНИЙ КЛІЄНТ !!!";
				text3 = "!!! УВАГА !!!";
				text4 = "НАГАДУЄМО ВАМ, ЩО " + EndDateKeyPay;
				text5 = "ТЕРМІН ДІЇ ЛІЦЕНЗІЇЇ НА ПРРО 'ВЕБЧЕК' ЗАКІНЧУЄТЬСЯ.";
				text = "РАДИМО ВАМ ПРОДОВЖИТИ ТЕРМІН ДІЇ ЛІЦЕНЗІЇЇ, ЗАМОВИВШИ РАХУНОК НА НАШОМУ САЙТІ.";
				text6 = "ВСЬОГО НАЙКРАЩОГО!";
				text7 = "";
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = text2;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = text3;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = text4;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = text5;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = text;
			StrCheckR[StrCheck.Count() - 1] = "";
			if (Operators.CompareString(text6.Trim(), "", TextCompare: false) != 0)
			{
				ref string[] strCheck8 = ref StrCheck;
				strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR8 = ref StrCheckR;
				strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text6;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			if (Operators.CompareString(text7.Trim(), "", TextCompare: false) != 0)
			{
				ref string[] strCheck9 = ref StrCheck;
				strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR9 = ref StrCheckR;
				strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text7;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck10 = ref StrCheck;
			strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR10 = ref StrCheckR;
			strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void EndKey()
	{
		ref string[] strCheck = ref StrCheck;
		checked
		{
			strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR = ref StrCheckR;
			strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "!!! ШАНОВНИЙ КЛІЄНТ !!!";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "!!! УВАГА !!!";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "НАГАДУЄМО ВАМ, ЩО " + EndDateKeyPay;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ТЕРМІН ДІЇ ВАШОГО КЛЮЧА ЕЦП ЗАКІНЧУЄТЬСЯ.";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "РАДИМО ВАМ ОТРИМАТИ НОВИЙ КЛЮЧ ЕЦП У АЦСК З ЯКИМ ВИ СПІВПРАЦЮЄТЕ, ЗАРЕЄСТРУВАТИ НОВИЙ КЛЮЧ ДЛЯ КАСИРА В ЕЛЕКТРОННОМУ КАБІНЕТІ ПЛАТНИКА ТА ЗМІНИТИ В НАЛАШТУВАННЯХ ПРРО.";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "В ІНШОМУ ВИПАДКУ ПРРО ЗАБЛОКУЄТЬСЯ ЗА 36 ГОДИН ДО ДАТИ, ЩО ПЕРЕДУЄ ДАТІ " + EndDateKeyPay;
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ВСЬОГО НАЙКРАЩОГО!";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck10 = ref StrCheck;
			strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR10 = ref StrCheckR;
			strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "***************************";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void Perenos()
	{
		StrCheckN = new string[1];
		checked
		{
			int num = StrCheck.Count() - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Operators.CompareString(StrCheckR[i].Trim(), "", TextCompare: false) == 0)
				{
					StrokaCen(StrCheck[i], Dlstr);
				}
				else if (Operators.CompareString(StrCheckR[i], "---", TextCompare: false) == 0)
				{
					StrokaRazdela(Dlstr);
				}
				else
				{
					StrokaAll(StrCheck[i], StrCheckR[i], Dlstr);
				}
			}
		}
	}

	private string StrokaAll(string s1, string s2, int dl)
	{
		if (Operators.CompareString(s1, "", TextCompare: false) == 0)
		{
			return s1;
		}
		int length = s1.Length;
		int length2 = s2.Length;
		checked
		{
			string text;
			if (length + length2 > dl)
			{
				if (s1.Length > dl)
				{
					text = s1.Substring(0, dl);
				}
				else
				{
					text = s1 + Strings.Space(dl - length + 1);
					s1 = text;
				}
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN = ref StrCheckN;
				strCheckN = (string[])Utils.CopyArray(strCheckN, new string[StrCheckN.Count() + 1]);
				int num = s1.Length - dl;
				return StrokaAll(s1.Substring(s1.Length - num), s2, dl);
			}
			if (length + length2 < dl)
			{
				int number = dl - (length + length2);
				text = s1 + Strings.Space(number) + s2;
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN2 = ref StrCheckN;
				strCheckN2 = (string[])Utils.CopyArray(strCheckN2, new string[StrCheckN.Count() + 1]);
				return "";
			}
			text = s1 + s2;
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN3 = ref StrCheckN;
			strCheckN3 = (string[])Utils.CopyArray(strCheckN3, new string[StrCheckN.Count() + 1]);
			return "";
		}
	}

	private string StrokaCen(string s, int dl)
	{
		if (Operators.CompareString(s, "", TextCompare: false) == 0)
		{
			return s;
		}
		checked
		{
			string text;
			if (s.Length > dl)
			{
				text = s.Substring(0, dl);
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN = ref StrCheckN;
				strCheckN = (string[])Utils.CopyArray(strCheckN, new string[StrCheckN.Count() + 1]);
				int num = s.Length - dl;
				return StrokaCen(s.Substring(s.Length - num), dl);
			}
			if (s.Length < dl)
			{
				int num2 = dl - s.Length;
				int num3 = unchecked(num2 / 2);
				int number = num2 - num3;
				text = Strings.Space(num3) + s + Strings.Space(number);
				StrCheckN[StrCheckN.Count() - 1] = text;
				ref string[] strCheckN2 = ref StrCheckN;
				strCheckN2 = (string[])Utils.CopyArray(strCheckN2, new string[StrCheckN.Count() + 1]);
				return "";
			}
			text = s;
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN3 = ref StrCheckN;
			strCheckN3 = (string[])Utils.CopyArray(strCheckN3, new string[StrCheckN.Count() + 1]);
			return "";
		}
	}

	private string StrokaRazdela(int dl, string raz = "-")
	{
		string text = "";
		checked
		{
			for (int i = 1; i <= dl; i++)
			{
				text += raz;
			}
			StrCheckN[StrCheckN.Count() - 1] = text;
			ref string[] strCheckN = ref StrCheckN;
			strCheckN = (string[])Utils.CopyArray(strCheckN, new string[StrCheckN.Count() + 1]);
			return text;
		}
	}

	private void Druk_Click(object sender, EventArgs e)
	{
		PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
		try
		{
			base.TopMost = false;
			PrintDocument1.Print();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			if (PrintDialog1.ShowDialog() == DialogResult.OK)
			{
				All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
				PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
				PrintDocument1.Print();
				All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
			}
			ProjectData.ClearProjectError();
		}
		if (All.A.FullVersion)
		{
			Application.DoEvents();
			Close();
		}
	}

	private void Rb1_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "PrinterWidth", 57);
		if (All.A.ecoPrint & All.A.FullVersion)
		{
			Dlstr = 39;
		}
		else
		{
			Dlstr = 29;
		}
		ResCheck();
		Application.DoEvents();
		checked
		{
			if (All.A.AutomatPrintCheck & CloseForm & All.A.FullVersion)
			{
				base.Top = -9000;
				PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
				try
				{
					base.TopMost = false;
					PrintDocument1.Print();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (PrintDialog1.ShowDialog() == DialogResult.OK)
					{
						All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
						PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
						PrintDocument1.Print();
						All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
					}
					ProjectData.ClearProjectError();
				}
				Application.DoEvents();
				Close();
			}
			else if (base.Top < 0)
			{
				base.Top = unchecked(Screen.PrimaryScreen.Bounds.Height / 2) - unchecked(base.Height / 2);
			}
		}
	}

	private void Rb2_CheckedChanged(object sender, EventArgs e)
	{
		All.f.IntigerWriteFN(All.A.FN, "PrinterWidth", 80);
		if (All.A.ecoPrint & All.A.FullVersion)
		{
			Dlstr = 50;
		}
		else
		{
			Dlstr = 40;
		}
		ResCheck();
		Application.DoEvents();
		checked
		{
			if (All.A.AutomatPrintCheck & CloseForm & All.A.FullVersion)
			{
				base.Top = -9000;
				PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
				try
				{
					base.TopMost = false;
					PrintDocument1.Print();
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					if (PrintDialog1.ShowDialog() == DialogResult.OK)
					{
						All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
						PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
						PrintDocument1.Print();
						All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
					}
					ProjectData.ClearProjectError();
				}
				Application.DoEvents();
				Close();
			}
			else if (base.Top < 0)
			{
				base.Top = unchecked(Screen.PrimaryScreen.Bounds.Height / 2) - unchecked(base.Height / 2);
			}
		}
	}

	private void ЗакритиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void XMLtoDim(string xmlCheck, bool vosvrat = false, string OnOf = "онлайн", string MACcur = "МакМакМак")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string[] array = new string[101];
			int num = 0;
			do
			{
				array[num] = "";
				num++;
			}
			while (num <= 100);
			try
			{
				array[0] = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@email").Value + "'";
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				array[0] = "0";
				ProjectData.ClearProjectError();
			}
			string text = "";
			try
			{
				text = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@taxa").Value;
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				text = "";
				ProjectData.ClearProjectError();
			}
			bool flag = false;
			bool flag2 = false;
			TypDopTeg typDopTeg = default(TypDopTeg);
			try
			{
				typDopTeg.PA = xmlDocument.SelectSingleNode("rq/dat/c/e/@pa").Value;
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				typDopTeg.PA = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PB = xmlDocument.SelectSingleNode("rq/dat/c/e/@pb").Value;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				typDopTeg.PB = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PC = xmlDocument.SelectSingleNode("rq/dat/c/e/@pc").Value;
			}
			catch (Exception ex11)
			{
				ProjectData.SetProjectError(ex11);
				Exception ex12 = ex11;
				typDopTeg.PC = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PD = xmlDocument.SelectSingleNode("rq/dat/c/e/@pd").Value;
			}
			catch (Exception ex13)
			{
				ProjectData.SetProjectError(ex13);
				Exception ex14 = ex13;
				typDopTeg.PD = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PE = xmlDocument.SelectSingleNode("rq/dat/c/e/@pe").Value;
			}
			catch (Exception ex15)
			{
				ProjectData.SetProjectError(ex15);
				Exception ex16 = ex15;
				typDopTeg.PE = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PSNM = xmlDocument.SelectSingleNode("rq/dat/c/e/@psnm").Value;
			}
			catch (Exception ex17)
			{
				ProjectData.SetProjectError(ex17);
				Exception ex18 = ex17;
				typDopTeg.PSNM = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.RRN = xmlDocument.SelectSingleNode("rq/dat/c/e/@rrn").Value;
			}
			catch (Exception ex19)
			{
				ProjectData.SetProjectError(ex19);
				Exception ex20 = ex19;
				typDopTeg.RRN = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PF = xmlDocument.SelectSingleNode("rq/dat/c/e/@pf").Value;
			}
			catch (Exception ex21)
			{
				ProjectData.SetProjectError(ex21);
				Exception ex22 = ex21;
				typDopTeg.PF = "";
				ProjectData.ClearProjectError();
			}
			num = 1;
			do
			{
				if (!flag)
				{
					string xpath = "rq/dat/c/webcheck/@up" + num;
					try
					{
						array[num] = xmlDocument.SelectSingleNode(xpath).Value;
						if (num == 1)
						{
							ref string[] strCheck2 = ref StrCheck;
							strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
							ref string[] strCheckR2 = ref StrCheckR;
							strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = "";
							StrCheckR[StrCheck.Count() - 1] = "---";
						}
						ref string[] strCheck3 = ref StrCheck;
						strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR3 = ref StrCheckR;
						strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = array[num];
						StrCheckR[StrCheck.Count() - 1] = "#";
					}
					catch (Exception ex23)
					{
						ProjectData.SetProjectError(ex23);
						Exception ex24 = ex23;
						array[num] = "";
						flag = true;
						ProjectData.ClearProjectError();
					}
				}
				if (!flag2)
				{
					string xpath2 = "rq/dat/c/webcheck/@dn" + num;
					try
					{
						array[num + 50] = xmlDocument.SelectSingleNode(xpath2).Value;
					}
					catch (Exception ex25)
					{
						ProjectData.SetProjectError(ex25);
						Exception ex26 = ex25;
						array[num + 50] = "";
						flag2 = true;
						ProjectData.ClearProjectError();
					}
				}
				if (unchecked(flag && flag2))
				{
					break;
				}
				num++;
			}
			while (num <= 50);
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("p");
			int num2 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num3 = num2;
			for (int i = 0; i <= num3; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr = All.d.GetParametrToString(outerXml, "q", "p").ReturnStr;
				returnStr = (All.A.PointRegion ? Strings.Replace(returnStr, ",", ".") : Strings.Replace(returnStr, ".", ","));
				double num4 = 0.0;
				double num5 = 0.0;
				double num6 = 0.0;
				string text2 = "";
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = All.KolvoVes(returnStr) + " x";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				double num7 = All.StrToDouble(returnStr);
				num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				num5 = num7 * num4;
				string returnStr2 = All.d.GetParametrToString(outerXml, "cd", "p").ReturnStr;
				TypProductName typProductName = All.DecoderProductName(All.d.GetParametrToString(outerXml, "nm", "p", RegUpLow: true).ReturnStr);
				if (typProductName.Uktzed.Length > 0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typProductName.Uktzed;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (returnStr2.Length > 0)
				{
					ref string[] strCheck7 = ref StrCheck;
					strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR7 = ref StrCheckR;
					strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = returnStr2;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typProductName.Excisestamp.Length > 0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typProductName.Excisestamp;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				ref string[] strCheck9 = ref StrCheck;
				strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR9 = ref StrCheckR;
				strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = typProductName.Name;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num5.ToString());
				string returnStr3 = All.d.GetParametrToString(outerXml, "tx", "p").ReturnStr;
				returnStr3 = All.PayTax.NUMtoABC(returnStr3);
				StrCheckR[StrCheck.Count() - 1] = StrCheckR[StrCheck.Count() - 1] + " " + returnStr3;
				double num8 = All.StrToDouble(All.d.GetParametrToString(outerXml, "sm", "p").ReturnStr);
				num5 = All.StrToDouble(All.Bablo(num5.ToString()));
				num6 = All.StrToDouble(All.Bablo(num8 - num5));
				double num9 = All.StrToDouble(All.d.GetParametrToString(outerXml, "avans", "p").ReturnStr);
				string returnStr4 = All.d.GetParametrToString(outerXml, "avansm", "p").ReturnStr;
				num6 += num9;
				string text3 = "";
				if (num6 > 0.0)
				{
					text2 = "НАЦIНКА";
					text3 = "";
				}
				else if (num6 < 0.0)
				{
					text2 = "ЗНИЖКА";
					text3 = "-";
				}
				if (Operators.CompareString(text2, "", TextCompare: false) != 0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = text2;
					StrCheckR[StrCheck.Count() - 1] = text3 + All.Bablo(Math.Abs(num6)) + " " + returnStr3;
				}
				if (num9 > 0.0)
				{
					ref string[] strCheck11 = ref StrCheck;
					strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR11 = ref StrCheckR;
					strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = returnStr4.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = "-" + All.Bablo(num9);
				}
			}
			bool flag3 = false;
			if (All.A.Showacquiring)
			{
				if (typDopTeg.PA.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PA;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PB.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck13 = ref StrCheck;
					strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR13 = ref StrCheckR;
					strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ТЕРМIНАЛ: " + typDopTeg.PB;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PF.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОМІСІЯ: " + typDopTeg.PF + " грн";
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PC.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck15 = ref StrCheck;
					strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR15 = ref StrCheckR;
					strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PC;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PD.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck16 = ref StrCheck;
					strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR16 = ref StrCheckR;
					strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ЕПЗ: " + typDopTeg.PD;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PSNM.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck17 = ref StrCheck;
					strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR17 = ref StrCheckR;
					strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ПЛАТIЖНА СИСТЕМА:" + typDopTeg.PSNM;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PE.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck18 = ref StrCheck;
					strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR18 = ref StrCheckR;
					strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД АВТОРИЗАЦІЇ:" + typDopTeg.PE;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.RRN.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck19 = ref StrCheck;
					strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR19 = ref StrCheckR;
					strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД ТРАНЗ.:" + typDopTeg.RRN;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
			}
			if (array[51].Trim().Length > 0 && !flag3)
			{
				flag3 = DrawRazdel();
			}
			num = 51;
			do
			{
				if (array[num].Trim().Length > 0)
				{
					ref string[] strCheck20 = ref StrCheck;
					strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR20 = ref StrCheckR;
					strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[num];
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				num++;
			}
			while (num <= 100);
			ref string[] strCheck21 = ref StrCheck;
			strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR21 = ref StrCheckR;
			strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num2 = elementsByTagName.Count - 1;
			string text4 = "";
			string[,] array2 = new string[num2 + 1, 4];
			double num10 = 0.0;
			double num11 = 0.0;
			double num12 = 0.0;
			int num13 = num2;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr5 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr5, "", TextCompare: false) != 0)
				{
					array2[i, 0] = returnStr5.ToUpper();
					array2[i, 1] = All.d.GetParametrToString(outerXml, "sm", "m").ReturnStr;
					array2[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array2[i, 3] = " ";
					if (!Versioned.IsNumeric(array2[i, 2]))
					{
						array2[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array2[i, 2]) == 2) & (Operators.CompareString(array2[i, 0], "КАРТКА", TextCompare: false) == 0))
					{
						array2[i, 2] = "3";
					}
					if (Conversions.ToInteger(array2[i, 2]) > 2)
					{
						array2[i, 2] = "1";
					}
					if (Operators.CompareString(array2[i, 2], "0", TextCompare: false) == 0)
					{
						num10 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(array2[i, 2], "1", TextCompare: false) == 0)
					{
						num11 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(array2[i, 2], "2", TextCompare: false) == 0)
					{
						num12 += All.StrToDouble(array2[i, 1]);
					}
					if (Operators.CompareString(returnStr5.ToLower(), "готівка", TextCompare: false) == 0 && Operators.CompareString(All.d.GetParametrToString(outerXml, "rm", "m").ReturnStr, "", TextCompare: false) != 0)
					{
						text4 = All.Bablo(All.d.GetParametrToString(outerXml, "rm", "m").ReturnStr);
					}
				}
			}
			string text5 = All.f.GetString("Global", "CheckPayForms");
			if (Operators.CompareString(text5, "", TextCompare: false) == 0)
			{
				text5 = "2";
				All.f.WriteString("Global", "CheckPayForms", text5);
			}
			if (num10 > 0.0)
			{
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num10) + " грн";
			}
			if (num11 > 0.0)
			{
				if (Operators.CompareString(text5, "2", TextCompare: false) != 0)
				{
					ref string[] strCheck23 = ref StrCheck;
					strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR23 = ref StrCheckR;
					strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num11) + " грн";
				}
				if (!vosvrat)
				{
					int num14 = num2;
					for (int i = 0; i <= num14; i++)
					{
						if (((Conversions.ToInteger(array2[i, 2]) == 1) | (Conversions.ToInteger(array2[i, 2]) > 2)) && All.StrToDouble(array2[i, 1]) > 0.0)
						{
							if (Operators.CompareString(text5, "2", TextCompare: false) == 0)
							{
								ref string[] strCheck24 = ref StrCheck;
								strCheck24 = (string[])Utils.CopyArray(strCheck24, new string[StrCheck.Count() + 1]);
								ref string[] strCheckR24 = ref StrCheckR;
								strCheckR24 = (string[])Utils.CopyArray(strCheckR24, new string[StrCheck.Count() + 1]);
								StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]) + " грн";
							}
							ref string[] strCheck25 = ref StrCheck;
							strCheck25 = (string[])Utils.CopyArray(strCheck25, new string[StrCheck.Count() + 1]);
							ref string[] strCheckR25 = ref StrCheckR;
							strCheckR25 = (string[])Utils.CopyArray(strCheckR25, new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = array2[i, 3] + array2[i, 0];
							if (Operators.CompareString(text5, "1", TextCompare: false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]);
							}
							else
							{
								StrCheckR[StrCheck.Count() - 1] = "#";
							}
						}
					}
				}
				else if (Operators.CompareString(text5, "2", TextCompare: false) == 0)
				{
					ref string[] strCheck26 = ref StrCheck;
					strCheck26 = (string[])Utils.CopyArray(strCheck26, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR26 = ref StrCheckR;
					strCheckR26 = (string[])Utils.CopyArray(strCheckR26, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num11) + " грн";
				}
			}
			if (num12 > 0.0)
			{
				if (Operators.CompareString(text5, "2", TextCompare: false) != 0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray(strCheck27, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray(strCheckR27, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num12) + " грн";
				}
				if (!vosvrat)
				{
					int num15 = num2;
					for (int i = 0; i <= num15; i++)
					{
						if (Conversions.ToInteger(array2[i, 2]) == 2 && All.StrToDouble(array2[i, 1]) > 0.0)
						{
							if (Operators.CompareString(text5, "2", TextCompare: false) == 0)
							{
								ref string[] strCheck28 = ref StrCheck;
								strCheck28 = (string[])Utils.CopyArray(strCheck28, new string[StrCheck.Count() + 1]);
								ref string[] strCheckR28 = ref StrCheckR;
								strCheckR28 = (string[])Utils.CopyArray(strCheckR28, new string[StrCheck.Count() + 1]);
								StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]) + " грн";
							}
							ref string[] strCheck29 = ref StrCheck;
							strCheck29 = (string[])Utils.CopyArray(strCheck29, new string[StrCheck.Count() + 1]);
							ref string[] strCheckR29 = ref StrCheckR;
							strCheckR29 = (string[])Utils.CopyArray(strCheckR29, new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = array2[i, 3] + array2[i, 0];
							if (Operators.CompareString(text5, "1", TextCompare: false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(array2[i, 1]);
							}
							else
							{
								StrCheckR[StrCheck.Count() - 1] = "#";
							}
						}
					}
				}
				else if (Operators.CompareString(text5, "2", TextCompare: false) == 0)
				{
					ref string[] strCheck30 = ref StrCheck;
					strCheck30 = (string[])Utils.CopyArray(strCheck30, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR30 = ref StrCheckR;
					strCheckR30 = (string[])Utils.CopyArray(strCheckR30, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(num12) + " грн";
				}
			}
			ref string[] strCheck31 = ref StrCheck;
			strCheck31 = (string[])Utils.CopyArray(strCheck31, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR31 = ref StrCheckR;
			strCheckR31 = (string[])Utils.CopyArray(strCheckR31, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr6 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/e").ReturnStr;
			if (Operators.CompareString(returnStr6, "", TextCompare: false) != 0)
			{
				ref string[] strCheck32 = ref StrCheck;
				strCheck32 = (string[])Utils.CopyArray(strCheck32, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR32 = ref StrCheckR;
				strCheckR32 = (string[])Utils.CopyArray(strCheckR32, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СУМА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr6) + " грн";
			}
			elementsByTagName = xmlDocument.GetElementsByTagName("tx");
			num2 = elementsByTagName.Count - 1;
			double num16 = 0.0;
			string text6 = "";
			bool flag4 = false;
			int num17 = num2;
			for (int i = 0; i <= num17; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr7 = All.d.GetParametrToString(outerXml, "tx", "tx").ReturnStr;
				if (Operators.CompareString(returnStr7, "", TextCompare: false) != 0)
				{
					if ((Conversions.ToInteger(returnStr7) < 4) | (Conversions.ToInteger(returnStr7) > 7))
					{
						if (!unchecked(Operators.CompareString(returnStr7, "1", TextCompare: false) == 0 && flag4))
						{
							ref string[] strCheck33 = ref StrCheck;
							strCheck33 = (string[])Utils.CopyArray(strCheck33, new string[StrCheck.Count() + 1]);
							ref string[] strCheckR33 = ref StrCheckR;
							strCheckR33 = (string[])Utils.CopyArray(strCheckR33, new string[StrCheck.Count() + 1]);
						}
						switch (returnStr7)
						{
						case "8":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=НЕОПОД.";
							break;
						case "9":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=БЕЗ ПДВ";
							break;
						case "10":
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=НЕ ОПОДАТКОВУЄТЬСЯ";
							break;
						default:
							StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC(returnStr7) + "=" + All.PayTax.get_TaxPRC(Conversions.ToInteger(returnStr7)) + "%";
							break;
						}
						if (Operators.CompareString(returnStr7, "1", TextCompare: false) == 0)
						{
							if (Operators.CompareString(text.Trim(), "", TextCompare: false) == 0)
							{
								StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txsm", "tx").ReturnStr);
							}
							else if (!flag4)
							{
								flag4 = true;
								StrCheckR[StrCheck.Count() - 1] = text;
							}
						}
						else
						{
							StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txsm", "tx").ReturnStr);
						}
					}
					else if (Operators.CompareString(text.Trim(), "", TextCompare: false) != 0 && ((Operators.CompareString(returnStr7, "4", TextCompare: false) == 0) | (Operators.CompareString(returnStr7, "6", TextCompare: false) == 0)) && !flag4)
					{
						flag4 = true;
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray(strCheck34, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray(strCheckR34, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = "ПДВ " + All.PayTax.NUMtoABC("1") + "=" + All.PayTax.get_TaxPRC(1) + "%";
						StrCheckR[StrCheck.Count() - 1] = text;
					}
				}
				string returnStr8 = All.d.GetParametrToString(outerXml, "dtpr", "tx").ReturnStr;
				if (All.StrToDouble(returnStr8) > 0.0)
				{
					string returnStr9 = All.d.GetParametrToString(outerXml, "dtsm", "tx").ReturnStr;
					num16 += All.StrToDouble(returnStr9);
					if (All.StrToDouble(returnStr8) == 7.5)
					{
						text6 = "ПФ  Д=7.5%";
					}
					else if (All.StrToDouble(returnStr8) == 5.0)
					{
						text6 = "АКЦ.ПОД. Г=5%";
					}
				}
			}
			if (Operators.CompareString(text6, "", TextCompare: false) != 0)
			{
				ref string[] strCheck35 = ref StrCheck;
				strCheck35 = (string[])Utils.CopyArray(strCheck35, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR35 = ref StrCheckR;
				strCheckR35 = (string[])Utils.CopyArray(strCheckR35, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text6;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num16);
			}
			string returnStr10 = All.d.GetParametrToString(xmlCheck, "smp", "rq/dat/c/m").ReturnStr;
			string returnStr11 = All.d.GetParametrToString(xmlCheck, "smm", "rq/dat/c/m").ReturnStr;
			string text7 = returnStr6;
			if (returnStr10.Length > 0)
			{
				ref string[] strCheck36 = ref StrCheck;
				strCheck36 = (string[])Utils.CopyArray(strCheck36, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR36 = ref StrCheckR;
				strCheckR36 = (string[])Utils.CopyArray(strCheckR36, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОКРУГЛЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr10);
				text7 = All.Bablo(All.StrToDouble(returnStr6) + All.StrToDouble(returnStr10));
			}
			else if (returnStr11.Length > 0)
			{
				ref string[] strCheck37 = ref StrCheck;
				strCheck37 = (string[])Utils.CopyArray(strCheck37, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR37 = ref StrCheckR;
				strCheckR37 = (string[])Utils.CopyArray(strCheckR37, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОКРУГЛЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr11);
				text7 = All.Bablo(All.StrToDouble(returnStr6) - All.StrToDouble(returnStr11));
			}
			if (!vosvrat)
			{
				ref string[] strCheck38 = ref StrCheck;
				strCheck38 = (string[])Utils.CopyArray(strCheck38, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR38 = ref StrCheckR;
				strCheckR38 = (string[])Utils.CopyArray(strCheckR38, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ДО СПЛАТИ";
				StrCheckR[StrCheck.Count() - 1] = text7 + " грн";
			}
			if (Operators.CompareString(text4, "", TextCompare: false) != 0)
			{
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray(strCheck39, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray(strCheckR39, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "РЕШТА";
				StrCheckR[StrCheck.Count() - 1] = text4 + " грн";
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray(strCheck40, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray(strCheckR40, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = "ЧЕК № " + TB2.Text;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray(strCheck41, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray(strCheckR41, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			DataTimePr = StrCheck[StrCheck.Count() - 1] + StrCheckR[StrCheck.Count() - 1];
			MacPr = MACcur;
			FiChPr = TB2.Text;
			SumPr = All.Bablo(returnStr6);
			FnPr = All.A.FN;
			ref string[] strCheck42 = ref StrCheck;
			strCheck42 = (string[])Utils.CopyArray(strCheck42, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR42 = ref StrCheckR;
			strCheckR42 = (string[])Utils.CopyArray(strCheckR42, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "HotGamesBest";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck43 = ref StrCheck;
			strCheck43 = (string[])Utils.CopyArray(strCheck43, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR43 = ref StrCheckR;
			strCheckR43 = (string[])Utils.CopyArray(strCheckR43, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			if (Operators.CompareString(OnOf, "офлайн", TextCompare: false) == 0)
			{
				ref string[] strCheck44 = ref StrCheck;
				strCheck44 = (string[])Utils.CopyArray(strCheck44, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR44 = ref StrCheckR;
				strCheckR44 = (string[])Utils.CopyArray(strCheckR44, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = MACcur;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray(strCheck45, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray(strCheckR45, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			if (vosvrat)
			{
				ref string[] strCheck46 = ref StrCheck;
				strCheck46 = (string[])Utils.CopyArray(strCheck46, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR46 = ref StrCheckR;
				strCheckR46 = (string[])Utils.CopyArray(strCheckR46, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ВИДАТКОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0))
			{
				ref string[] strCheck47 = ref StrCheck;
				strCheck47 = (string[])Utils.CopyArray(strCheck47, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR47 = ref StrCheckR;
				strCheckR47 = (string[])Utils.CopyArray(strCheckR47, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else if (!vosvrat)
			{
				ref string[] strCheck48 = ref StrCheck;
				strCheck48 = (string[])Utils.CopyArray(strCheck48, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR48 = ref StrCheckR;
				strCheckR48 = (string[])Utils.CopyArray(strCheckR48, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private string Okruglit(string m)
	{
		return All.Bablo(Strings.FormatNumber(All.StrToDouble(m), 1));
	}

	private void XMLtoDimEPZ(string xmlCheck, string OnOf = "онлайн", string MACcur = "МакМакМак")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string[] array = new string[101];
			int num = 0;
			do
			{
				array[num] = "";
				num++;
			}
			while (num <= 100);
			try
			{
				array[0] = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@email").Value + "'";
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				array[0] = "0";
				ProjectData.ClearProjectError();
			}
			try
			{
				_ = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@taxa").Value;
			}
			catch (Exception ex5)
			{
				ProjectData.SetProjectError(ex5);
				Exception ex6 = ex5;
				ProjectData.ClearProjectError();
			}
			bool flag = false;
			bool flag2 = false;
			TypDopTeg typDopTeg = default(TypDopTeg);
			try
			{
				typDopTeg.PA = xmlDocument.SelectSingleNode("rq/dat/c/e/@pa").Value;
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				typDopTeg.PA = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PB = xmlDocument.SelectSingleNode("rq/dat/c/e/@pb").Value;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				typDopTeg.PB = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PC = xmlDocument.SelectSingleNode("rq/dat/c/e/@pc").Value;
			}
			catch (Exception ex11)
			{
				ProjectData.SetProjectError(ex11);
				Exception ex12 = ex11;
				typDopTeg.PC = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PD = xmlDocument.SelectSingleNode("rq/dat/c/e/@pd").Value;
			}
			catch (Exception ex13)
			{
				ProjectData.SetProjectError(ex13);
				Exception ex14 = ex13;
				typDopTeg.PD = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PE = xmlDocument.SelectSingleNode("rq/dat/c/e/@pe").Value;
			}
			catch (Exception ex15)
			{
				ProjectData.SetProjectError(ex15);
				Exception ex16 = ex15;
				typDopTeg.PE = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PSNM = xmlDocument.SelectSingleNode("rq/dat/c/e/@psnm").Value;
			}
			catch (Exception ex17)
			{
				ProjectData.SetProjectError(ex17);
				Exception ex18 = ex17;
				typDopTeg.PSNM = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.RRN = xmlDocument.SelectSingleNode("rq/dat/c/e/@rrn").Value;
			}
			catch (Exception ex19)
			{
				ProjectData.SetProjectError(ex19);
				Exception ex20 = ex19;
				typDopTeg.RRN = "";
				ProjectData.ClearProjectError();
			}
			try
			{
				typDopTeg.PF = xmlDocument.SelectSingleNode("rq/dat/c/e/@pf").Value;
			}
			catch (Exception ex21)
			{
				ProjectData.SetProjectError(ex21);
				Exception ex22 = ex21;
				typDopTeg.PF = "";
				ProjectData.ClearProjectError();
			}
			num = 1;
			do
			{
				if (!flag)
				{
					string xpath = "rq/dat/c/webcheck/@up" + num;
					try
					{
						array[num] = xmlDocument.SelectSingleNode(xpath).Value;
						if (num == 1)
						{
							ref string[] strCheck2 = ref StrCheck;
							strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
							ref string[] strCheckR2 = ref StrCheckR;
							strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
							StrCheck[StrCheck.Count() - 1] = "";
							StrCheckR[StrCheck.Count() - 1] = "---";
						}
						ref string[] strCheck3 = ref StrCheck;
						strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR3 = ref StrCheckR;
						strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = array[num];
						StrCheckR[StrCheck.Count() - 1] = "#";
					}
					catch (Exception ex23)
					{
						ProjectData.SetProjectError(ex23);
						Exception ex24 = ex23;
						array[num] = "";
						flag = true;
						ProjectData.ClearProjectError();
					}
				}
				if (!flag2)
				{
					string xpath2 = "rq/dat/c/webcheck/@dn" + num;
					try
					{
						array[num + 50] = xmlDocument.SelectSingleNode(xpath2).Value;
					}
					catch (Exception ex25)
					{
						ProjectData.SetProjectError(ex25);
						Exception ex26 = ex25;
						array[num + 50] = "";
						flag2 = true;
						ProjectData.ClearProjectError();
					}
				}
				if (unchecked(flag && flag2))
				{
					break;
				}
				num++;
			}
			while (num <= 50);
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("p");
			int num2 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			int num3 = num2;
			for (int i = 0; i <= num3; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr = All.d.GetParametrToString(outerXml, "q", "p").ReturnStr;
				returnStr = (All.A.PointRegion ? Strings.Replace(returnStr, ",", ".") : Strings.Replace(returnStr, ".", ","));
				double num4 = 0.0;
				double num5 = 0.0;
				double num6 = All.StrToDouble(returnStr);
				num4 = All.StrToDouble(All.d.GetParametrToString(outerXml, "prc", "p").ReturnStr);
				num5 = num6 * num4;
				All.d.GetParametrToString(outerXml, "cd", "p");
				All.DecoderProductName(All.d.GetParametrToString(outerXml, "nm", "p", RegUpLow: true).ReturnStr);
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЯМ ЕПЗ   ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num5.ToString()) + " ГРН";
			}
			bool flag3 = false;
			if (All.A.Showacquiring)
			{
				if (typDopTeg.PA.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PA;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PB.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck7 = ref StrCheck;
					strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR7 = ref StrCheckR;
					strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ТЕРМIНАЛ: " + typDopTeg.PB;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PF.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОМІСІЯ: " + typDopTeg.PF + " грн";
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PC.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck9 = ref StrCheck;
					strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR9 = ref StrCheckR;
					strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = typDopTeg.PC;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PD.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ЕПЗ: " + typDopTeg.PD;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PSNM.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck11 = ref StrCheck;
					strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR11 = ref StrCheckR;
					strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ПЛАТIЖНА СИСТЕМА:" + typDopTeg.PSNM;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.PE.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД АВТОРИЗАЦІЇ:" + typDopTeg.PE;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				if (typDopTeg.RRN.Trim().Length > 0)
				{
					if (!flag3)
					{
						flag3 = DrawRazdel();
					}
					ref string[] strCheck13 = ref StrCheck;
					strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR13 = ref StrCheckR;
					strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "КОД ТРАНЗ.:" + typDopTeg.RRN;
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
			}
			if (array[51].Trim().Length > 0 && !flag3)
			{
				flag3 = DrawRazdel();
			}
			num = 51;
			do
			{
				if (array[num].Trim().Length > 0)
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[num];
					StrCheckR[StrCheck.Count() - 1] = "#";
				}
				num++;
			}
			while (num <= 100);
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/e").ReturnStr;
			elementsByTagName = xmlDocument.GetElementsByTagName("tx");
			_ = elementsByTagName.Count - 1;
			ref string[] strCheck16 = ref StrCheck;
			strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR16 = ref StrCheckR;
			strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = TB2.Text;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck17 = ref StrCheck;
			strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR17 = ref StrCheckR;
			strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			DataTimePr = StrCheck[StrCheck.Count() - 1] + StrCheckR[StrCheck.Count() - 1];
			MacPr = MACcur;
			FiChPr = TB2.Text;
			SumPr = All.Bablo(returnStr2);
			FnPr = All.A.FN;
			ref string[] strCheck18 = ref StrCheck;
			strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR18 = ref StrCheckR;
			strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "HotGamesBest";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			if (Operators.CompareString(OnOf, "офлайн", TextCompare: false) == 0)
			{
				ref string[] strCheck20 = ref StrCheck;
				strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR20 = ref StrCheckR;
				strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = MACcur;
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck21 = ref StrCheck;
			strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR21 = ref StrCheckR;
			strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0))
			{
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else
			{
				ref string[] strCheck23 = ref StrCheck;
				strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR23 = ref StrCheckR;
				strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЧЕК ВИДАЧІ КОШТІВ";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private bool DrawRazdel()
	{
		ref string[] strCheck = ref StrCheck;
		checked
		{
			strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR = ref StrCheckR;
			strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			return true;
		}
	}

	private string LongToData(string LongDT, bool ForLink = false)
	{
		if (LongDT.Length != 14)
		{
			return "дата";
		}
		if (!ForLink)
		{
			return Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]) + "." + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + "." + Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]);
		}
		return Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]) + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]);
	}

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}

	private string TimeToTimeWWW(string TimeCheck)
	{
		return Conversions.ToString(TimeCheck[0]) + Conversions.ToString(TimeCheck[1]) + Conversions.ToString(TimeCheck[3]) + Conversions.ToString(TimeCheck[4]);
	}

	private void XMLtoDimS(string xmlCheck, string OnOf = "онлайн")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/i").ReturnStr;
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "sm", "rq/dat/c/o").ReturnStr;
			if (Operators.CompareString(returnStr.Trim(), "", TextCompare: false) == 0)
			{
				returnStr = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/c/i").ReturnStr;
			}
			if (Operators.CompareString(returnStr2.Trim(), "", TextCompare: false) == 0)
			{
				returnStr2 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/c/o").ReturnStr;
			}
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			if (Operators.CompareString(returnStr, "", TextCompare: false) != 0)
			{
				ref string[] strCheck3 = ref StrCheck;
				strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR3 = ref StrCheckR;
				strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr);
			}
			else if (Operators.CompareString(returnStr2, "", TextCompare: false) != 0)
			{
				ref string[] strCheck4 = ref StrCheck;
				strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR4 = ref StrCheckR;
				strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr2);
			}
			else
			{
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = TB2.Text;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck10 = ref StrCheck;
			strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR10 = ref StrCheckR;
			strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "С Л У Ж Б О В И Й   Ч Е К";
			StrCheckR[StrCheck.Count() - 1] = "";
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0))
			{
				ref string[] strCheck11 = ref StrCheck;
				strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR11 = ref StrCheckR;
				strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
			else
			{
				ref string[] strCheck12 = ref StrCheck;
				strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR12 = ref StrCheckR;
				strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
			}
		}
	}

	private void XMLtoAll(string xmlCheck, string OnOf = "онлайн")
	{
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck);
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "t", "rq/dat/c").ReturnStr;
			int num = 0;
			if (Versioned.IsNumeric(returnStr))
			{
				num = Conversions.ToInteger(returnStr);
			}
			if (num > 100)
			{
				num -= 100;
			}
			returnStr = num.ToString();
			ref string[] strCheck2 = ref StrCheck;
			strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR2 = ref StrCheckR;
			strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			switch (returnStr)
			{
			case "8":
			{
				ref string[] strCheck7 = ref StrCheck;
				strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR7 = ref StrCheckR;
				strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ВІДКРИТТЯ ЗМІНИ ПРРО";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "12":
			{
				ref string[] strCheck6 = ref StrCheck;
				strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR6 = ref StrCheckR;
				strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАПИТ ДІАПАЗОНУ РЕЗЕРВНИХ НОМЕРІВ ДЛЯ РОБОТИ В РЕЖИМІ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "9":
			{
				ref string[] strCheck5 = ref StrCheck;
				strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR5 = ref StrCheckR;
				strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОЧАТОК ПЕРЕВЕДЕННЯ ПРРО В РЕЖИМ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			case "10":
			{
				ref string[] strCheck4 = ref StrCheck;
				strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR4 = ref StrCheckR;
				strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАВЕРШЕННЯ РЕЖИМУ ОФЛАЙН";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			default:
			{
				ref string[] strCheck3 = ref StrCheck;
				strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR3 = ref StrCheckR;
				strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				break;
			}
			}
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = TB2.Text;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck10 = ref StrCheck;
			strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR10 = ref StrCheckR;
			strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck12 = ref StrCheck;
			strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR12 = ref StrCheckR;
			strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "С Л У Ж Б О В И Й   Ч Е К";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimX(string xmlCheck)
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", TextCompare: false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "X ЗВIT #" + returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string text3 = All.d.GetParametrToString(xmlCheck, "ni", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", TextCompare: false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			bool flag = false;
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr2 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr2, "", TextCompare: false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr2.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric(array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr2.ToLower(), "готівка", TextCompare: false) == 0)
				{
					num = num6;
					flag = true;
					text4 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text4, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text5, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text6, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text7 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text7, "", TextCompare: false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr3, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr3.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr4 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr4.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr4);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck12 = ref StrCheck;
						strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR12 = ref StrCheckR;
						strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr3.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr5 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr5.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr5);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck13 = ref StrCheck;
						strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR13 = ref StrCheckR;
						strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr3.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr6 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr6, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr7 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr6.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr8;
					if (Versioned.IsNumeric(text2))
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "DTI", returnStr8);
					}
					else
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "TXI", returnStr8);
					}
					if (Operators.CompareString(returnStr8.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr6.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr9;
					if (Versioned.IsNumeric(text2))
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "DTI", returnStr9);
					}
					else
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "TXI", returnStr9);
					}
					if (Operators.CompareString(returnStr9.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck18 = ref StrCheck;
				strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR18 = ref StrCheckR;
				strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr6.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck20 = ref StrCheck;
			strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR20 = ref StrCheckR;
			strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck21 = ref StrCheck;
				strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR21 = ref StrCheckR;
				strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray(strCheck24, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray(strCheckR24, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			text3 = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", TextCompare: false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray(strCheck25, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray(strCheckR25, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr10 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr10, "", TextCompare: false) != 0)
				{
					array[i, 0] = returnStr10.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric(array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr10.ToLower(), "готівка", TextCompare: false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray(strCheck26, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray(strCheckR26, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray(strCheck27, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray(strCheckR27, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck28 = ref StrCheck;
			strCheck28 = (string[])Utils.CopyArray(strCheck28, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR28 = ref StrCheckR;
			strCheckR28 = (string[])Utils.CopyArray(strCheckR28, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck29 = ref StrCheck;
					strCheck29 = (string[])Utils.CopyArray(strCheck29, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR29 = ref StrCheckR;
					strCheckR29 = (string[])Utils.CopyArray(strCheckR29, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck30 = ref StrCheck;
			strCheck30 = (string[])Utils.CopyArray(strCheck30, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR30 = ref StrCheckR;
			strCheckR30 = (string[])Utils.CopyArray(strCheckR30, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck31 = ref StrCheck;
					strCheck31 = (string[])Utils.CopyArray(strCheck31, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR31 = ref StrCheckR;
					strCheckR31 = (string[])Utils.CopyArray(strCheckR31, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck32 = ref StrCheck;
			strCheck32 = (string[])Utils.CopyArray(strCheck32, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR32 = ref StrCheckR;
			strCheckR32 = (string[])Utils.CopyArray(strCheckR32, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr11 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr11, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr11.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr12 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr12.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr12);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck33 = ref StrCheck;
						strCheck33 = (string[])Utils.CopyArray(strCheck33, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR33 = ref StrCheckR;
						strCheckR33 = (string[])Utils.CopyArray(strCheckR33, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr11.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr13 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr13.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr13);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray(strCheck34, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray(strCheckR34, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck35 = ref StrCheck;
					strCheck35 = (string[])Utils.CopyArray(strCheck35, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR35 = ref StrCheckR;
					strCheckR35 = (string[])Utils.CopyArray(strCheckR35, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr11.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck36 = ref StrCheck;
			strCheck36 = (string[])Utils.CopyArray(strCheck36, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR36 = ref StrCheckR;
			strCheckR36 = (string[])Utils.CopyArray(strCheckR36, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr14 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr14, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr15 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr14.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "гб", TextCompare: false) == 0))
				{
					string text8 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text8.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(text8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck37 = ref StrCheck;
						strCheck37 = (string[])Utils.CopyArray(strCheck37, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR37 = ref StrCheckR;
						strCheckR37 = (string[])Utils.CopyArray(strCheckR37, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr14.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "дб", TextCompare: false) == 0))
				{
					string text9 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text9.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(text9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck38 = ref StrCheck;
						strCheck38 = (string[])Utils.CopyArray(strCheck38, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR38 = ref StrCheckR;
						strCheckR38 = (string[])Utils.CopyArray(strCheckR38, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray(strCheck39, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray(strCheckR39, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr14.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray(strCheck40, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray(strCheckR40, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray(strCheck41, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray(strCheckR41, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray(strCheck42, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray(strCheckR42, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
				ref string[] strCheck43 = ref StrCheck;
				strCheck43 = (string[])Utils.CopyArray(strCheck43, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR43 = ref StrCheckR;
				strCheckR43 = (string[])Utils.CopyArray(strCheckR43, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text7);
			}
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray(strCheck44, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray(strCheckR44, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr16 = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/z/io").ReturnStr;
			string returnStr17 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/z/io").ReturnStr;
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray(strCheck45, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray(strCheckR45, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr16);
			num += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck46 = ref StrCheck;
			strCheck46 = (string[])Utils.CopyArray(strCheck46, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR46 = ref StrCheckR;
			strCheckR46 = (string[])Utils.CopyArray(strCheckR46, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
			num -= All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			string returnStr18 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric(returnStr18))
			{
				num -= All.StrToDouble(returnStr18);
			}
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray(strCheck47, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray(strCheckR47, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА У СЕЙФІ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num.ToString());
			ref string[] strCheck48 = ref StrCheck;
			strCheck48 = (string[])Utils.CopyArray(strCheck48, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR48 = ref StrCheckR;
			strCheckR48 = (string[])Utils.CopyArray(strCheckR48, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr19 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric(returnStr19) && Conversions.ToInteger(returnStr19) > 0)
			{
				ref string[] strCheck49 = ref StrCheck;
				strCheck49 = (string[])Utils.CopyArray(strCheck49, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR49 = ref StrCheckR;
				strCheckR49 = (string[])Utils.CopyArray(strCheckR49, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
				ref string[] strCheck50 = ref StrCheck;
				strCheck50 = (string[])Utils.CopyArray(strCheck50, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR50 = ref StrCheckR;
				strCheckR50 = (string[])Utils.CopyArray(strCheckR50, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr19;
				ref string[] strCheck51 = ref StrCheck;
				strCheck51 = (string[])Utils.CopyArray(strCheck51, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR51 = ref StrCheckR;
				strCheckR51 = (string[])Utils.CopyArray(strCheckR51, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck52 = ref StrCheck;
			strCheck52 = (string[])Utils.CopyArray(strCheck52, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR52 = ref StrCheckR;
			strCheckR52 = (string[])Utils.CopyArray(strCheckR52, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck53 = ref StrCheck;
			strCheck53 = (string[])Utils.CopyArray(strCheck53, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR53 = ref StrCheckR;
			strCheckR53 = (string[])Utils.CopyArray(strCheckR53, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck54 = ref StrCheck;
			strCheck54 = (string[])Utils.CopyArray(strCheck54, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR54 = ref StrCheckR;
			strCheckR54 = (string[])Utils.CopyArray(strCheckR54, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = TB2.Text;
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimP(string xmlCheck)
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", TextCompare: false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПЕРІОДИЧНИЙ ЗВІТ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string text3 = "";
			string text4 = "";
			string text5 = "";
			string text6 = "";
			bool flag = false;
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "ns", "rq/dat/z").ReturnStr;
			string returnStr3 = All.d.GetParametrToString(xmlCheck, "ds", "rq/dat/z").ReturnStr;
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "З № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "ne", "rq/dat/z").ReturnStr;
			returnStr3 = All.d.GetParametrToString(xmlCheck, "de", "rq/dat/z").ReturnStr;
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ДО № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "all", "rq/dat/z").ReturnStr;
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ВСЬОГО Z ЗВІТІВ";
			StrCheckR[StrCheck.Count() - 1] = returnStr2;
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr4 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr4, "", TextCompare: false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr4.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric(array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr4.ToLower(), "готівка", TextCompare: false) == 0)
				{
					num = num6;
					flag = true;
					text3 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text3, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text4 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text4, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text5, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text6, "", TextCompare: false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck13 = ref StrCheck;
			strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR13 = ref StrCheckR;
			strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr5 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr5, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr5.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr6 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr6.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr6);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
				}
				else if ((Operators.CompareString(returnStr5.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr7 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr7.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr7);
						text = "ОБIГ ПФ  Д=7.5%";
					}
				}
				else
				{
					ref string[] strCheck16 = ref StrCheck;
					strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR16 = ref StrCheckR;
					strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr5.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			if (Operators.CompareString(text, "", TextCompare: false) != 0)
			{
				ref string[] strCheck17 = ref StrCheck;
				strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR17 = ref StrCheckR;
				strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
				num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck18 = ref StrCheck;
			strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR18 = ref StrCheckR;
			strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr8 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr8, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr9 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr8.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr10 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
					if (Operators.CompareString(returnStr10.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr10);
						text = "АКЦ.ПОД. Г=5%";
					}
					continue;
				}
				if ((Operators.CompareString(returnStr8.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "дб", TextCompare: false) == 0))
				{
					string text7 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr);
					if (Operators.CompareString(text7.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(text7);
						text = "ПДВ ПФ  Д=7.5%";
					}
					continue;
				}
				ref string[] strCheck19 = ref StrCheck;
				strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR19 = ref StrCheckR;
				strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr8.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr9);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			if (Operators.CompareString(text, "", TextCompare: false) != 0)
			{
				ref string[] strCheck20 = ref StrCheck;
				strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR20 = ref StrCheckR;
				strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck21 = ref StrCheck;
			strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR21 = ref StrCheckR;
			strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck22 = ref StrCheck;
			strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR22 = ref StrCheckR;
			strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck23 = ref StrCheck;
				strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR23 = ref StrCheckR;
				strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text3);
				ref string[] strCheck24 = ref StrCheck;
				strCheck24 = (string[])Utils.CopyArray(strCheck24, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR24 = ref StrCheckR;
				strCheckR24 = (string[])Utils.CopyArray(strCheckR24, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
			}
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray(strCheck25, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray(strCheckR25, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray(strCheck26, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray(strCheckR26, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr11 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr11, "", TextCompare: false) != 0)
				{
					array[i, 0] = returnStr11.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric(array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr11.ToLower(), "готівка", TextCompare: false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck27 = ref StrCheck;
			strCheck27 = (string[])Utils.CopyArray(strCheck27, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR27 = ref StrCheckR;
			strCheckR27 = (string[])Utils.CopyArray(strCheckR27, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck28 = ref StrCheck;
					strCheck28 = (string[])Utils.CopyArray(strCheck28, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR28 = ref StrCheckR;
					strCheckR28 = (string[])Utils.CopyArray(strCheckR28, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck29 = ref StrCheck;
			strCheck29 = (string[])Utils.CopyArray(strCheck29, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR29 = ref StrCheckR;
			strCheckR29 = (string[])Utils.CopyArray(strCheckR29, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck30 = ref StrCheck;
					strCheck30 = (string[])Utils.CopyArray(strCheck30, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR30 = ref StrCheckR;
					strCheckR30 = (string[])Utils.CopyArray(strCheckR30, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck31 = ref StrCheck;
			strCheck31 = (string[])Utils.CopyArray(strCheck31, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR31 = ref StrCheckR;
			strCheckR31 = (string[])Utils.CopyArray(strCheckR31, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck32 = ref StrCheck;
					strCheck32 = (string[])Utils.CopyArray(strCheck32, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR32 = ref StrCheckR;
					strCheckR32 = (string[])Utils.CopyArray(strCheckR32, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck33 = ref StrCheck;
			strCheck33 = (string[])Utils.CopyArray(strCheck33, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR33 = ref StrCheckR;
			strCheckR33 = (string[])Utils.CopyArray(strCheckR33, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr12 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr12, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr12.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr12.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr13 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr13.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr13);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
				}
				else if ((Operators.CompareString(returnStr12.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr12.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr14 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr14.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr14);
						text = "ОБIГ ПФ  Д=7.5%";
					}
				}
				else
				{
					ref string[] strCheck34 = ref StrCheck;
					strCheck34 = (string[])Utils.CopyArray(strCheck34, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR34 = ref StrCheckR;
					strCheckR34 = (string[])Utils.CopyArray(strCheckR34, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr12.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			if (Operators.CompareString(text, "", TextCompare: false) != 0)
			{
				ref string[] strCheck35 = ref StrCheck;
				strCheck35 = (string[])Utils.CopyArray(strCheck35, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR35 = ref StrCheckR;
				strCheckR35 = (string[])Utils.CopyArray(strCheckR35, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
				num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck36 = ref StrCheck;
			strCheck36 = (string[])Utils.CopyArray(strCheck36, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR36 = ref StrCheckR;
			strCheckR36 = (string[])Utils.CopyArray(strCheckR36, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr15 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr15, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr16 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr15.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr15.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr17 = All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr;
					if (Operators.CompareString(returnStr17.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr17);
						text = "АКЦ.ПОД. Г=5%";
					}
					continue;
				}
				if ((Operators.CompareString(returnStr15.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr15.ToLower(), "дб", TextCompare: false) == 0))
				{
					string text8 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text8.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(text8);
						text = "ПДВ ПФ  Д=7.5%";
					}
					continue;
				}
				ref string[] strCheck37 = ref StrCheck;
				strCheck37 = (string[])Utils.CopyArray(strCheck37, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR37 = ref StrCheckR;
				strCheckR37 = (string[])Utils.CopyArray(strCheckR37, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr15.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr15.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr15.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr15.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr15.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr15.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr15.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr16);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			if (Operators.CompareString(text, "", TextCompare: false) != 0)
			{
				ref string[] strCheck38 = ref StrCheck;
				strCheck38 = (string[])Utils.CopyArray(strCheck38, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR38 = ref StrCheckR;
				strCheckR38 = (string[])Utils.CopyArray(strCheckR38, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = text;
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck39 = ref StrCheck;
			strCheck39 = (string[])Utils.CopyArray(strCheck39, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR39 = ref StrCheckR;
			strCheckR39 = (string[])Utils.CopyArray(strCheckR39, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray(strCheck40, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray(strCheckR40, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck41 = ref StrCheck;
				strCheck41 = (string[])Utils.CopyArray(strCheck41, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR41 = ref StrCheckR;
				strCheckR41 = (string[])Utils.CopyArray(strCheckR41, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray(strCheck42, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray(strCheckR42, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
			}
			ref string[] strCheck43 = ref StrCheck;
			strCheck43 = (string[])Utils.CopyArray(strCheck43, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR43 = ref StrCheckR;
			strCheckR43 = (string[])Utils.CopyArray(strCheckR43, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray(strCheck44, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray(strCheckR44, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray(strCheck45, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray(strCheckR45, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck46 = ref StrCheck;
			strCheck46 = (string[])Utils.CopyArray(strCheck46, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR46 = ref StrCheckR;
			strCheckR46 = (string[])Utils.CopyArray(strCheckR46, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = TB2.Text;
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimPeriod(string xmlCheck)
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", TextCompare: false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПЕРІОДИЧНИЙ ЗВІТ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string returnStr2 = All.d.GetParametrToString(xmlCheck, "ns", "rq/dat/z").ReturnStr;
			string returnStr3 = All.d.GetParametrToString(xmlCheck, "ds", "rq/dat/z").ReturnStr;
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "З № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "ne", "rq/dat/z").ReturnStr;
			returnStr3 = All.d.GetParametrToString(xmlCheck, "de", "rq/dat/z").ReturnStr;
			ref string[] strCheck6 = ref StrCheck;
			strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR6 = ref StrCheckR;
			strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ДО № " + returnStr2;
			StrCheckR[StrCheck.Count() - 1] = returnStr3;
			returnStr2 = All.d.GetParametrToString(xmlCheck, "all", "rq/dat/z").ReturnStr;
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ВСЬОГО Z ЗВІТІВ";
			StrCheckR[StrCheck.Count() - 1] = returnStr2;
			ref string[] strCheck8 = ref StrCheck;
			strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR8 = ref StrCheckR;
			strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string text3 = "";
			string text4 = "";
			string text5 = "";
			string text6 = "";
			bool flag = false;
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr4 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr4, "", TextCompare: false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr4.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric(array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr4.ToLower(), "готівка", TextCompare: false) == 0)
				{
					num = num6;
					flag = true;
					text3 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text3, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text4 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text4, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text5, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text6, "", TextCompare: false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck12 = ref StrCheck;
					strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR12 = ref StrCheckR;
					strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck13 = ref StrCheck;
			strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR13 = ref StrCheckR;
			strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr5 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr5, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr5.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr6 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr6.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr6);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr6);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr5.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr5.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr7 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr7.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr7);
						text = "ОБIГ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck18 = ref StrCheck;
					strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR18 = ref StrCheckR;
					strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr5.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr8 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr8, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr9 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr8.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr10 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
					if (Operators.CompareString(returnStr10.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr10);
						text = "АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck20 = ref StrCheck;
						strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR20 = ref StrCheckR;
						strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr10);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr8.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr8.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr11 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
					if (Operators.CompareString(returnStr11.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr11);
						text = "ПДВ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck21 = ref StrCheck;
						strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR21 = ref StrCheckR;
						strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr11);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr8.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr8.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr8.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr9);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray(strCheck24, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray(strCheckR24, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck25 = ref StrCheck;
				strCheck25 = (string[])Utils.CopyArray(strCheck25, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR25 = ref StrCheckR;
				strCheckR25 = (string[])Utils.CopyArray(strCheckR25, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text3);
				ref string[] strCheck26 = ref StrCheck;
				strCheck26 = (string[])Utils.CopyArray(strCheck26, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR26 = ref StrCheckR;
				strCheckR26 = (string[])Utils.CopyArray(strCheckR26, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
			}
			ref string[] strCheck27 = ref StrCheck;
			strCheck27 = (string[])Utils.CopyArray(strCheck27, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR27 = ref StrCheckR;
			strCheckR27 = (string[])Utils.CopyArray(strCheckR27, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck28 = ref StrCheck;
			strCheck28 = (string[])Utils.CopyArray(strCheck28, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR28 = ref StrCheckR;
			strCheckR28 = (string[])Utils.CopyArray(strCheckR28, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr12 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr12, "", TextCompare: false) != 0)
				{
					array[i, 0] = returnStr12.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric(array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr12.ToLower(), "готівка", TextCompare: false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck29 = ref StrCheck;
			strCheck29 = (string[])Utils.CopyArray(strCheck29, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR29 = ref StrCheckR;
			strCheckR29 = (string[])Utils.CopyArray(strCheckR29, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck30 = ref StrCheck;
					strCheck30 = (string[])Utils.CopyArray(strCheck30, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR30 = ref StrCheckR;
					strCheckR30 = (string[])Utils.CopyArray(strCheckR30, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck31 = ref StrCheck;
			strCheck31 = (string[])Utils.CopyArray(strCheck31, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR31 = ref StrCheckR;
			strCheckR31 = (string[])Utils.CopyArray(strCheckR31, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck32 = ref StrCheck;
					strCheck32 = (string[])Utils.CopyArray(strCheck32, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR32 = ref StrCheckR;
					strCheckR32 = (string[])Utils.CopyArray(strCheckR32, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck33 = ref StrCheck;
			strCheck33 = (string[])Utils.CopyArray(strCheck33, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR33 = ref StrCheckR;
			strCheckR33 = (string[])Utils.CopyArray(strCheckR33, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck34 = ref StrCheck;
					strCheck34 = (string[])Utils.CopyArray(strCheck34, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR34 = ref StrCheckR;
					strCheckR34 = (string[])Utils.CopyArray(strCheckR34, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck35 = ref StrCheck;
			strCheck35 = (string[])Utils.CopyArray(strCheck35, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR35 = ref StrCheckR;
			strCheckR35 = (string[])Utils.CopyArray(strCheckR35, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr13 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr13, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr13.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr13.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr14 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr14.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr14);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck36 = ref StrCheck;
						strCheck36 = (string[])Utils.CopyArray(strCheck36, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR36 = ref StrCheckR;
						strCheckR36 = (string[])Utils.CopyArray(strCheckR36, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr14);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr13.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr13.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr15 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr15.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr15);
						text = "ОБIГ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck37 = ref StrCheck;
						strCheck37 = (string[])Utils.CopyArray(strCheck37, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR37 = ref StrCheckR;
						strCheckR37 = (string[])Utils.CopyArray(strCheckR37, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck38 = ref StrCheck;
					strCheck38 = (string[])Utils.CopyArray(strCheck38, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR38 = ref StrCheckR;
					strCheckR38 = (string[])Utils.CopyArray(strCheckR38, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr13.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck39 = ref StrCheck;
			strCheck39 = (string[])Utils.CopyArray(strCheck39, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR39 = ref StrCheckR;
			strCheckR39 = (string[])Utils.CopyArray(strCheckR39, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr16 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr16, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr17 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr16.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr16.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr18 = All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr;
					if (Operators.CompareString(returnStr18.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr18);
						text = "АКЦ.ПОД. Г=5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck40 = ref StrCheck;
						strCheck40 = (string[])Utils.CopyArray(strCheck40, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR40 = ref StrCheckR;
						strCheckR40 = (string[])Utils.CopyArray(strCheckR40, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr16.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr16.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr19 = All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr;
					if (Operators.CompareString(returnStr19.Trim(), "", TextCompare: false) != 0)
					{
						num4 += All.StrToDouble(returnStr19);
						text = "ПДВ ПФ  Д=7.5%";
					}
					if (Operators.CompareString(text, "", TextCompare: false) != 0)
					{
						ref string[] strCheck41 = ref StrCheck;
						strCheck41 = (string[])Utils.CopyArray(strCheck41, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR41 = ref StrCheckR;
						strCheckR41 = (string[])Utils.CopyArray(strCheckR41, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr19);
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray(strCheck42, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray(strCheckR42, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr16.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr16.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr16.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr16.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck43 = ref StrCheck;
			strCheck43 = (string[])Utils.CopyArray(strCheck43, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR43 = ref StrCheckR;
			strCheckR43 = (string[])Utils.CopyArray(strCheckR43, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray(strCheck44, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray(strCheckR44, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck45 = ref StrCheck;
				strCheck45 = (string[])Utils.CopyArray(strCheck45, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR45 = ref StrCheckR;
				strCheckR45 = (string[])Utils.CopyArray(strCheckR45, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
				ref string[] strCheck46 = ref StrCheck;
				strCheck46 = (string[])Utils.CopyArray(strCheck46, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR46 = ref StrCheckR;
				strCheckR46 = (string[])Utils.CopyArray(strCheckR46, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
			}
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray(strCheck47, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray(strCheckR47, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr20 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			string returnStr21 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric(returnStr20) && Conversions.ToInteger(returnStr20) > 0)
			{
				ref string[] strCheck48 = ref StrCheck;
				strCheck48 = (string[])Utils.CopyArray(strCheck48, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR48 = ref StrCheckR;
				strCheckR48 = (string[])Utils.CopyArray(strCheckR48, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr21);
				ref string[] strCheck49 = ref StrCheck;
				strCheck49 = (string[])Utils.CopyArray(strCheck49, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR49 = ref StrCheckR;
				strCheckR49 = (string[])Utils.CopyArray(strCheckR49, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr20;
				ref string[] strCheck50 = ref StrCheck;
				strCheck50 = (string[])Utils.CopyArray(strCheck50, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR50 = ref StrCheckR;
				strCheckR50 = (string[])Utils.CopyArray(strCheckR50, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck51 = ref StrCheck;
			strCheck51 = (string[])Utils.CopyArray(strCheck51, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR51 = ref StrCheckR;
			strCheckR51 = (string[])Utils.CopyArray(strCheckR51, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck52 = ref StrCheck;
			strCheck52 = (string[])Utils.CopyArray(strCheck52, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR52 = ref StrCheckR;
			strCheckR52 = (string[])Utils.CopyArray(strCheckR52, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck53 = ref StrCheck;
			strCheck53 = (string[])Utils.CopyArray(strCheck53, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR53 = ref StrCheckR;
			strCheckR53 = (string[])Utils.CopyArray(strCheckR53, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = TB2.Text;
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void XMLtoDimZ(string xmlCheck, string OnOf = "онлайн")
	{
		double num = 0.0;
		double num2 = 0.0;
		double num3 = 0.0;
		double num4 = 0.0;
		string text = "";
		string text2 = "";
		XmlDocument xmlDocument = new XmlDocument();
		checked
		{
			try
			{
				xmlDocument.LoadXml(xmlCheck.ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ref string[] strCheck = ref StrCheck;
				strCheck = (string[])Utils.CopyArray(strCheck, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR = ref StrCheckR;
				strCheckR = (string[])Utils.CopyArray(strCheckR, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				ProjectData.ClearProjectError();
				return;
			}
			string returnStr = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").ReturnStr;
			if (Operators.CompareString(returnStr, "", TextCompare: false) == 0)
			{
				ref string[] strCheck2 = ref StrCheck;
				strCheck2 = (string[])Utils.CopyArray(strCheck2, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR2 = ref StrCheckR;
				strCheckR2 = (string[])Utils.CopyArray(strCheckR2, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ПОМИЛКА";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck3 = ref StrCheck;
			strCheck3 = (string[])Utils.CopyArray(strCheck3, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR3 = ref StrCheckR;
			strCheckR3 = (string[])Utils.CopyArray(strCheckR3, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "Z ЗВIT #" + returnStr;
			StrCheckR[StrCheck.Count() - 1] = "";
			string text3 = All.d.GetParametrToString(xmlCheck, "ni", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", TextCompare: false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck4 = ref StrCheck;
			strCheck4 = (string[])Utils.CopyArray(strCheck4, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR4 = ref StrCheckR;
			strCheckR4 = (string[])Utils.CopyArray(strCheckR4, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			string text4 = "";
			string text5 = "";
			string text6 = "";
			string text7 = "";
			bool flag = false;
			XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("m");
			int num5 = elementsByTagName.Count - 1;
			XmlDocument xmlDocument2 = new XmlDocument();
			string[,] array = new string[num5 + 1, 4];
			double num6 = 0.0;
			double num7 = 0.0;
			double num8 = 0.0;
			int num9 = num5;
			for (int i = 0; i <= num9; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr2 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr2, "", TextCompare: false) == 0)
				{
					continue;
				}
				array[i, 0] = returnStr2.ToUpper();
				array[i, 1] = All.d.GetParametrToString(outerXml, "smi", "m").ReturnStr;
				array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
				array[i, 3] = All.PayU;
				if (!Versioned.IsNumeric(array[i, 2]))
				{
					array[i, 2] = "3";
				}
				if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
				{
					array[i, 2] = "3";
				}
				if (Conversions.ToInteger(array[i, 2]) > 2)
				{
					array[i, 2] = "1";
				}
				if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
				{
					num6 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
				{
					num7 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
				{
					num8 += All.StrToDouble(array[i, 1]);
				}
				if (Operators.CompareString(returnStr2.ToLower(), "готівка", TextCompare: false) == 0)
				{
					num = num6;
					flag = true;
					text4 = All.d.GetParametrToString(outerXml, "smim", "m").ReturnStr;
					if (Operators.CompareString(text4, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text5 = All.d.GetParametrToString(outerXml, "smip", "m").ReturnStr;
					if (Operators.CompareString(text5, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text6 = All.d.GetParametrToString(outerXml, "smom", "m").ReturnStr;
					if (Operators.CompareString(text6, "", TextCompare: false) == 0)
					{
						flag = false;
					}
					text7 = All.d.GetParametrToString(outerXml, "smop", "m").ReturnStr;
					if (Operators.CompareString(text7, "", TextCompare: false) == 0)
					{
						flag = false;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck5 = ref StrCheck;
			strCheck5 = (string[])Utils.CopyArray(strCheck5, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR5 = ref StrCheckR;
			strCheckR5 = (string[])Utils.CopyArray(strCheckR5, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num10 = num5;
			for (int i = 0; i <= num10; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck6 = ref StrCheck;
					strCheck6 = (string[])Utils.CopyArray(strCheck6, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR6 = ref StrCheckR;
					strCheckR6 = (string[])Utils.CopyArray(strCheckR6, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck7 = ref StrCheck;
			strCheck7 = (string[])Utils.CopyArray(strCheck7, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR7 = ref StrCheckR;
			strCheckR7 = (string[])Utils.CopyArray(strCheckR7, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num11 = num5;
			for (int i = 0; i <= num11; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck8 = ref StrCheck;
					strCheck8 = (string[])Utils.CopyArray(strCheck8, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR8 = ref StrCheckR;
					strCheckR8 = (string[])Utils.CopyArray(strCheckR8, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck9 = ref StrCheck;
			strCheck9 = (string[])Utils.CopyArray(strCheck9, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR9 = ref StrCheckR;
			strCheckR9 = (string[])Utils.CopyArray(strCheckR9, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num12 = num5;
			for (int i = 0; i <= num12; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck10 = ref StrCheck;
					strCheck10 = (string[])Utils.CopyArray(strCheck10, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR10 = ref StrCheckR;
					strCheckR10 = (string[])Utils.CopyArray(strCheckR10, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck11 = ref StrCheck;
			strCheck11 = (string[])Utils.CopyArray(strCheck11, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR11 = ref StrCheckR;
			strCheckR11 = (string[])Utils.CopyArray(strCheckR11, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num13 = num5;
			for (int i = 0; i <= num13; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr3 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr3, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr3.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr4 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr4.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr4);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck12 = ref StrCheck;
						strCheck12 = (string[])Utils.CopyArray(strCheck12, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR12 = ref StrCheckR;
						strCheckR12 = (string[])Utils.CopyArray(strCheckR12, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr3.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr3.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr5 = All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr;
					if (Operators.CompareString(returnStr5.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr5);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck13 = ref StrCheck;
						strCheck13 = (string[])Utils.CopyArray(strCheck13, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR13 = ref StrCheckR;
						strCheckR13 = (string[])Utils.CopyArray(strCheckR13, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck14 = ref StrCheck;
					strCheck14 = (string[])Utils.CopyArray(strCheck14, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR14 = ref StrCheckR;
					strCheckR14 = (string[])Utils.CopyArray(strCheckR14, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr3.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smi", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck15 = ref StrCheck;
			strCheck15 = (string[])Utils.CopyArray(strCheck15, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR15 = ref StrCheckR;
			strCheckR15 = (string[])Utils.CopyArray(strCheckR15, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num14 = num5;
			for (int i = 0; i <= num14; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr6 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr6, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr7 = All.d.GetParametrToString(outerXml, "wchkain", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr6.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr8;
					if (Versioned.IsNumeric(text2))
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "DTI", returnStr8);
					}
					else
					{
						returnStr8 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ГА или ГБ", "TXI", returnStr8);
					}
					if (Operators.CompareString(returnStr8.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck16 = ref StrCheck;
						strCheck16 = (string[])Utils.CopyArray(strCheck16, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR16 = ref StrCheckR;
						strCheckR16 = (string[])Utils.CopyArray(strCheckR16, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr6.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr6.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr9;
					if (Versioned.IsNumeric(text2))
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "dti", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "DTI", returnStr9);
					}
					else
					{
						returnStr9 = All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr;
						All.Lg.SaveTextToLog("ДА или ДБ", "TXI", returnStr9);
					}
					if (Operators.CompareString(returnStr9.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck17 = ref StrCheck;
						strCheck17 = (string[])Utils.CopyArray(strCheck17, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR17 = ref StrCheckR;
						strCheckR17 = (string[])Utils.CopyArray(strCheckR17, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck18 = ref StrCheck;
				strCheck18 = (string[])Utils.CopyArray(strCheck18, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR18 = ref StrCheckR;
				strCheckR18 = (string[])Utils.CopyArray(strCheckR18, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr6.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr6.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr6.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr7);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txi", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck19 = ref StrCheck;
			strCheck19 = (string[])Utils.CopyArray(strCheck19, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR19 = ref StrCheckR;
			strCheckR19 = (string[])Utils.CopyArray(strCheckR19, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck20 = ref StrCheck;
			strCheck20 = (string[])Utils.CopyArray(strCheck20, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR20 = ref StrCheckR;
			strCheckR20 = (string[])Utils.CopyArray(strCheckR20, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck21 = ref StrCheck;
				strCheck21 = (string[])Utils.CopyArray(strCheck21, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR21 = ref StrCheckR;
				strCheckR21 = (string[])Utils.CopyArray(strCheckR21, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text4);
				ref string[] strCheck22 = ref StrCheck;
				strCheck22 = (string[])Utils.CopyArray(strCheck22, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR22 = ref StrCheckR;
				strCheckR22 = (string[])Utils.CopyArray(strCheckR22, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text5);
			}
			ref string[] strCheck23 = ref StrCheck;
			strCheck23 = (string[])Utils.CopyArray(strCheck23, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR23 = ref StrCheckR;
			strCheckR23 = (string[])Utils.CopyArray(strCheckR23, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			ref string[] strCheck24 = ref StrCheck;
			strCheck24 = (string[])Utils.CopyArray(strCheck24, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR24 = ref StrCheckR;
			strCheckR24 = (string[])Utils.CopyArray(strCheckR24, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОВЕРНЕНI";
			StrCheckR[StrCheck.Count() - 1] = "";
			text3 = All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z/nc").ReturnStr;
			if (Operators.CompareString(text3.Trim(), "", TextCompare: false) == 0)
			{
				text3 = "0";
			}
			ref string[] strCheck25 = ref StrCheck;
			strCheck25 = (string[])Utils.CopyArray(strCheck25, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR25 = ref StrCheckR;
			strCheckR25 = (string[])Utils.CopyArray(strCheckR25, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЧЕКIВ";
			StrCheckR[StrCheck.Count() - 1] = text3;
			elementsByTagName = xmlDocument.GetElementsByTagName("m");
			num5 = elementsByTagName.Count - 1;
			array = new string[num5 + 1, 4];
			num6 = 0.0;
			num7 = 0.0;
			num8 = 0.0;
			int num15 = num5;
			for (int i = 0; i <= num15; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr10 = All.d.GetParametrToString(outerXml, "nm", "m").ReturnStr;
				if (Operators.CompareString(returnStr10, "", TextCompare: false) != 0)
				{
					array[i, 0] = returnStr10.ToUpper();
					array[i, 1] = All.d.GetParametrToString(outerXml, "smo", "m").ReturnStr;
					array[i, 2] = All.d.GetParametrToString(outerXml, "t", "m").ReturnStr;
					array[i, 3] = All.PayU;
					if (!Versioned.IsNumeric(array[i, 2]))
					{
						array[i, 2] = "3";
					}
					if ((Conversions.ToInteger(array[i, 2]) == 2) & (Operators.CompareString(array[i, 0], "КАРТКА", TextCompare: false) == 0))
					{
						array[i, 2] = "3";
					}
					if (Conversions.ToInteger(array[i, 2]) > 2)
					{
						array[i, 2] = "1";
					}
					if (Operators.CompareString(array[i, 2], "0", TextCompare: false) == 0)
					{
						num6 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "1", TextCompare: false) == 0)
					{
						num7 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(array[i, 2], "2", TextCompare: false) == 0)
					{
						num8 += All.StrToDouble(array[i, 1]);
					}
					if (Operators.CompareString(returnStr10.ToLower(), "готівка", TextCompare: false) == 0)
					{
						num -= num6;
					}
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if ((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2))
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			for (int i = num5; i >= 0; i += -1)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2)
				{
					array[i, 3] = All.PayD;
					break;
				}
			}
			ref string[] strCheck26 = ref StrCheck;
			strCheck26 = (string[])Utils.CopyArray(strCheck26, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR26 = ref StrCheckR;
			strCheckR26 = (string[])Utils.CopyArray(strCheckR26, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num6);
			int num16 = num5;
			for (int i = 0; i <= num16; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 0 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck27 = ref StrCheck;
					strCheck27 = (string[])Utils.CopyArray(strCheck27, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR27 = ref StrCheckR;
					strCheckR27 = (string[])Utils.CopyArray(strCheckR27, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck28 = ref StrCheck;
			strCheck28 = (string[])Utils.CopyArray(strCheck28, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR28 = ref StrCheckR;
			strCheckR28 = (string[])Utils.CopyArray(strCheckR28, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "БЕЗГОТІВКОВА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num7);
			int num17 = num5;
			for (int i = 0; i <= num17; i++)
			{
				if (((Conversions.ToInteger(array[i, 2]) == 1) | (Conversions.ToInteger(array[i, 2]) > 2)) && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck29 = ref StrCheck;
					strCheck29 = (string[])Utils.CopyArray(strCheck29, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR29 = ref StrCheckR;
					strCheckR29 = (string[])Utils.CopyArray(strCheckR29, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck30 = ref StrCheck;
			strCheck30 = (string[])Utils.CopyArray(strCheck30, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR30 = ref StrCheckR;
			strCheckR30 = (string[])Utils.CopyArray(strCheckR30, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ІНШЕ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num8);
			int num18 = num5;
			for (int i = 0; i <= num18; i++)
			{
				if (Conversions.ToInteger(array[i, 2]) == 2 && All.StrToDouble(array[i, 1]) > 0.0)
				{
					ref string[] strCheck31 = ref StrCheck;
					strCheck31 = (string[])Utils.CopyArray(strCheck31, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR31 = ref StrCheckR;
					strCheckR31 = (string[])Utils.CopyArray(strCheckR31, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = array[i, 3] + array[i, 0];
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(array[i, 1]);
				}
			}
			ref string[] strCheck32 = ref StrCheck;
			strCheck32 = (string[])Utils.CopyArray(strCheck32, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR32 = ref StrCheckR;
			strCheckR32 = (string[])Utils.CopyArray(strCheckR32, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			elementsByTagName = xmlDocument.GetElementsByTagName("txs");
			num5 = elementsByTagName.Count - 1;
			num2 = 0.0;
			num4 = 0.0;
			text = "";
			int num19 = num5;
			for (int i = 0; i <= num19; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr11 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				if (Operators.CompareString(returnStr11, "", TextCompare: false) == 0)
				{
					continue;
				}
				if ((Operators.CompareString(returnStr11.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "гб", TextCompare: false) == 0))
				{
					string returnStr12 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr12.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr12);
						text = "ОБIГ АКЦ.ПОД. Г=5%";
						ref string[] strCheck33 = ref StrCheck;
						strCheck33 = (string[])Utils.CopyArray(strCheck33, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR33 = ref StrCheckR;
						strCheckR33 = (string[])Utils.CopyArray(strCheckR33, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else if ((Operators.CompareString(returnStr11.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr11.ToLower(), "дб", TextCompare: false) == 0))
				{
					string returnStr13 = All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr;
					if (Operators.CompareString(returnStr13.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(returnStr13);
						text = "ОБIГ ПФ  Д=7.5%";
						ref string[] strCheck34 = ref StrCheck;
						strCheck34 = (string[])Utils.CopyArray(strCheck34, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR34 = ref StrCheckR;
						strCheckR34 = (string[])Utils.CopyArray(strCheckR34, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
				}
				else
				{
					ref string[] strCheck35 = ref StrCheck;
					strCheck35 = (string[])Utils.CopyArray(strCheck35, new string[StrCheck.Count() + 1]);
					ref string[] strCheckR35 = ref StrCheckR;
					strCheckR35 = (string[])Utils.CopyArray(strCheckR35, new string[StrCheck.Count() + 1]);
					StrCheck[StrCheck.Count() - 1] = "ОБIГ " + returnStr11.ToUpper();
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "smo", "txs").ReturnStr);
					num2 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
				}
			}
			ref string[] strCheck36 = ref StrCheck;
			strCheck36 = (string[])Utils.CopyArray(strCheck36, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR36 = ref StrCheckR;
			strCheckR36 = (string[])Utils.CopyArray(strCheckR36, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ОБIГ ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			num3 = 0.0;
			num4 = 0.0;
			text = "";
			int num20 = num5;
			for (int i = 0; i <= num20; i++)
			{
				string outerXml = elementsByTagName[i].OuterXml;
				xmlDocument2.LoadXml(outerXml);
				string returnStr14 = All.d.GetParametrToString(outerXml, "n", "txs").ReturnStr;
				text2 = All.d.GetParametrToString(outerXml, "tx", "txs").ReturnStr;
				if (Operators.CompareString(returnStr14, "", TextCompare: false) == 0)
				{
					continue;
				}
				string returnStr15 = All.d.GetParametrToString(outerXml, "wchkaout", "txs").ReturnStr;
				if ((Operators.CompareString(returnStr14.ToLower(), "га", TextCompare: false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "гб", TextCompare: false) == 0))
				{
					string text8 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text8.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(text8);
						text = "АКЦ.ПОД. Г=5%";
						ref string[] strCheck37 = ref StrCheck;
						strCheck37 = (string[])Utils.CopyArray(strCheck37, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR37 = ref StrCheckR;
						strCheckR37 = (string[])Utils.CopyArray(strCheckR37, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				if ((Operators.CompareString(returnStr14.ToLower(), "да", TextCompare: false) == 0) | (Operators.CompareString(returnStr14.ToLower(), "дб", TextCompare: false) == 0))
				{
					string text9 = ((!Versioned.IsNumeric(text2)) ? All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr : All.d.GetParametrToString(outerXml, "dto", "txs").ReturnStr);
					if (Operators.CompareString(text9.Trim(), "", TextCompare: false) != 0)
					{
						num4 = All.StrToDouble(text9);
						text = "ПДВ ПФ  Д=7.5%";
						ref string[] strCheck38 = ref StrCheck;
						strCheck38 = (string[])Utils.CopyArray(strCheck38, new string[StrCheck.Count() + 1]);
						ref string[] strCheckR38 = ref StrCheckR;
						strCheckR38 = (string[])Utils.CopyArray(strCheckR38, new string[StrCheck.Count() + 1]);
						StrCheck[StrCheck.Count() - 1] = text;
						StrCheckR[StrCheck.Count() - 1] = All.Bablo(num4.ToString());
						num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
					}
					continue;
				}
				ref string[] strCheck39 = ref StrCheck;
				strCheck39 = (string[])Utils.CopyArray(strCheck39, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR39 = ref StrCheckR;
				strCheckR39 = (string[])Utils.CopyArray(strCheckR39, new string[StrCheck.Count() + 1]);
				if (Operators.CompareString(returnStr14.ToLower(), "е", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕОПОД.";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "ж", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=БЕЗ ПДВ";
				}
				else if (Operators.CompareString(returnStr14.ToLower(), "з", TextCompare: false) == 0)
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=НЕ ОПОДАТКОВУЄТЬСЯ";
				}
				else
				{
					StrCheck[StrCheck.Count() - 1] = "ПДВ " + returnStr14.ToUpper() + "=" + All.d.GetParametrToString(outerXml, "txpr", "txs").ReturnStr + "%";
				}
				if (!Versioned.IsNumeric(text2))
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				else if (Operators.CompareString(text2, "1", TextCompare: false) == 0)
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr15);
				}
				else
				{
					StrCheckR[StrCheck.Count() - 1] = All.Bablo(All.d.GetParametrToString(outerXml, "txo", "txs").ReturnStr);
				}
				num3 += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			}
			ref string[] strCheck40 = ref StrCheck;
			strCheck40 = (string[])Utils.CopyArray(strCheck40, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR40 = ref StrCheckR;
			strCheckR40 = (string[])Utils.CopyArray(strCheckR40, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПОДАТОК ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num3.ToString());
			ref string[] strCheck41 = ref StrCheck;
			strCheck41 = (string[])Utils.CopyArray(strCheck41, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR41 = ref StrCheckR;
			strCheckR41 = (string[])Utils.CopyArray(strCheckR41, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ЗАГ. СУМА ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num2.ToString());
			if (flag)
			{
				ref string[] strCheck42 = ref StrCheck;
				strCheck42 = (string[])Utils.CopyArray(strCheck42, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR42 = ref StrCheckR;
				strCheckR42 = (string[])Utils.CopyArray(strCheckR42, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В МЕНШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text6);
				ref string[] strCheck43 = ref StrCheck;
				strCheck43 = (string[])Utils.CopyArray(strCheck43, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR43 = ref StrCheckR;
				strCheckR43 = (string[])Utils.CopyArray(strCheckR43, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ЗАОК. В БIЛЬШИЙ БIК ";
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(text7);
			}
			ref string[] strCheck44 = ref StrCheck;
			strCheck44 = (string[])Utils.CopyArray(strCheck44, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR44 = ref StrCheckR;
			strCheckR44 = (string[])Utils.CopyArray(strCheckR44, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr16 = All.d.GetParametrToString(xmlCheck, "smi", "rq/dat/z/io").ReturnStr;
			string returnStr17 = All.d.GetParametrToString(xmlCheck, "smo", "rq/dat/z/io").ReturnStr;
			ref string[] strCheck45 = ref StrCheck;
			strCheck45 = (string[])Utils.CopyArray(strCheck45, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR45 = ref StrCheckR;
			strCheckR45 = (string[])Utils.CopyArray(strCheckR45, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВЕ ВНЕСЕННЯ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr16);
			num += All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck46 = ref StrCheck;
			strCheck46 = (string[])Utils.CopyArray(strCheck46, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR46 = ref StrCheckR;
			strCheckR46 = (string[])Utils.CopyArray(strCheckR46, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "СЛУЖБОВА ВИДАЧА";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr17);
			num -= All.StrToDouble(StrCheckR[StrCheck.Count() - 1]);
			string returnStr18 = All.d.GetParametrToString(xmlCheck, "epsm", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric(returnStr18))
			{
				num -= All.StrToDouble(returnStr18);
			}
			ref string[] strCheck47 = ref StrCheck;
			strCheck47 = (string[])Utils.CopyArray(strCheck47, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR47 = ref StrCheckR;
			strCheckR47 = (string[])Utils.CopyArray(strCheckR47, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ГОТІВКА У СЕЙФІ";
			StrCheckR[StrCheck.Count() - 1] = All.Bablo(num.ToString());
			ref string[] strCheck48 = ref StrCheck;
			strCheck48 = (string[])Utils.CopyArray(strCheck48, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR48 = ref StrCheckR;
			strCheckR48 = (string[])Utils.CopyArray(strCheckR48, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "";
			StrCheckR[StrCheck.Count() - 1] = "---";
			string returnStr19 = All.d.GetParametrToString(xmlCheck, "epc", "rq/dat/z/epz").ReturnStr;
			if (Versioned.IsNumeric(returnStr19) && Conversions.ToInteger(returnStr19) > 0)
			{
				ref string[] strCheck49 = ref StrCheck;
				strCheck49 = (string[])Utils.CopyArray(strCheck49, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR49 = ref StrCheckR;
				strCheckR49 = (string[])Utils.CopyArray(strCheckR49, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "сума по  видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = All.Bablo(returnStr18);
				ref string[] strCheck50 = ref StrCheck;
				strCheck50 = (string[])Utils.CopyArray(strCheck50, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR50 = ref StrCheckR;
				strCheckR50 = (string[])Utils.CopyArray(strCheckR50, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "Кількість операції  з видачі коштів ЕПЗ ".ToUpper();
				StrCheckR[StrCheck.Count() - 1] = returnStr19;
				ref string[] strCheck51 = ref StrCheck;
				strCheck51 = (string[])Utils.CopyArray(strCheck51, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR51 = ref StrCheckR;
				strCheckR51 = (string[])Utils.CopyArray(strCheckR51, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "";
				StrCheckR[StrCheck.Count() - 1] = "---";
			}
			ref string[] strCheck52 = ref StrCheck;
			strCheck52 = (string[])Utils.CopyArray(strCheck52, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR52 = ref StrCheckR;
			strCheckR52 = (string[])Utils.CopyArray(strCheckR52, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = TB2.Text;
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			ref string[] strCheck53 = ref StrCheck;
			strCheck53 = (string[])Utils.CopyArray(strCheck53, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR53 = ref StrCheckR;
			strCheckR53 = (string[])Utils.CopyArray(strCheckR53, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = LongToData(innerText);
			StrCheckR[StrCheck.Count() - 1] = LongToTime(innerText);
			DataWWW = LongToData(innerText, ForLink: true);
			TimeWWW = TimeToTimeWWW(StrCheckR[StrCheck.Count() - 1]);
			ref string[] strCheck54 = ref StrCheck;
			strCheck54 = (string[])Utils.CopyArray(strCheck54, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR54 = ref StrCheckR;
			strCheckR54 = (string[])Utils.CopyArray(strCheckR54, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = " ";
			StrCheckR[StrCheck.Count() - 1] = OnOf;
			ref string[] strCheck55 = ref StrCheck;
			strCheck55 = (string[])Utils.CopyArray(strCheck55, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR55 = ref StrCheckR;
			strCheckR55 = (string[])Utils.CopyArray(strCheckR55, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФН ПРРО";
			StrCheckR[StrCheck.Count() - 1] = All.A.FN;
			ref string[] strCheck56 = ref StrCheck;
			strCheck56 = (string[])Utils.CopyArray(strCheck56, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR56 = ref StrCheckR;
			strCheckR56 = (string[])Utils.CopyArray(strCheckR56, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФІСКАЛЬНИЙ ЗВІТ ДІЙСНИЙ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck57 = ref StrCheck;
			strCheck57 = (string[])Utils.CopyArray(strCheck57, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR57 = ref StrCheckR;
			strCheckR57 = (string[])Utils.CopyArray(strCheckR57, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = TB2.Text;
			StrCheckR[StrCheck.Count() - 1] = "";
			if ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0) | (Operators.CompareString(All.A.FN, "7000000512", TextCompare: false) == 0))
			{
				ref string[] strCheck58 = ref StrCheck;
				strCheck58 = (string[])Utils.CopyArray(strCheck58, new string[StrCheck.Count() + 1]);
				ref string[] strCheckR58 = ref StrCheckR;
				strCheckR58 = (string[])Utils.CopyArray(strCheckR58, new string[StrCheck.Count() + 1]);
				StrCheck[StrCheck.Count() - 1] = "ТЕСТОВИЙ ЧЕК";
				StrCheckR[StrCheck.Count() - 1] = "";
				return;
			}
			ref string[] strCheck59 = ref StrCheck;
			strCheck59 = (string[])Utils.CopyArray(strCheck59, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR59 = ref StrCheckR;
			strCheckR59 = (string[])Utils.CopyArray(strCheckR59, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ФIСКАЛЬНИЙ ЧЕК";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck60 = ref StrCheck;
			strCheck60 = (string[])Utils.CopyArray(strCheck60, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR60 = ref StrCheckR;
			strCheckR60 = (string[])Utils.CopyArray(strCheckR60, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "РЕГІСТРИ ДЕННИХ";
			StrCheckR[StrCheck.Count() - 1] = "";
			ref string[] strCheck61 = ref StrCheck;
			strCheck61 = (string[])Utils.CopyArray(strCheck61, new string[StrCheck.Count() + 1]);
			ref string[] strCheckR61 = ref StrCheckR;
			strCheckR61 = (string[])Utils.CopyArray(strCheckR61, new string[StrCheck.Count() + 1]);
			StrCheck[StrCheck.Count() - 1] = "ПІДСУМКІВ ОБНУЛЕНІ";
			StrCheckR[StrCheck.Count() - 1] = "";
		}
	}

	private void PrintDocument1_PrintPage(object sender, PrintPageEventArgs e)
	{
		int num = 0;
		string text = All.f.GetString("Global", "QrCode");
		int num2;
		try
		{
			if ((Dlstr == 29) | (Dlstr == 39))
			{
				e.Graphics.DrawImage(PrintLogo, 3, 3, 174, 45);
			}
			else
			{
				e.Graphics.DrawImage(PrintLogo, 36, 3, 174, 45);
			}
			num2 = 50;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			num2 = 0;
			ProjectData.ClearProjectError();
		}
		checked
		{
			if (All.A.ecoPrint & All.A.FullVersion)
			{
				int num3 = StrCheckN.Count() - 1;
				for (int i = 0; i <= num3; i++)
				{
					if (Operators.CompareString(StrCheckN[i], null, TextCompare: false) == 0)
					{
						StrCheckN[i] = "";
					}
					if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", TextCompare: false) != 0)
					{
						num = i * 8 + num2;
						e.Graphics.DrawString(StrCheckN[i], new System.Drawing.Font("Consolas", 6f), Brushes.Black, 0f, num);
					}
					else if ((TypWWW < 3) | (TypWWW == 8))
					{
						num = i * 8 + 6 + num2;
						if ((Dlstr == 29) | (Dlstr == 39))
						{
							e.Graphics.DrawImage(QrCode.Image, 36, num, 118, 118);
						}
						else
						{
							e.Graphics.DrawImage(QrCode.Image, 69, num, 118, 118);
						}
						num2 += 118;
					}
				}
			}
			else
			{
				int num4 = StrCheckN.Count() - 1;
				for (int i = 0; i <= num4; i++)
				{
					if (Operators.CompareString(StrCheckN[i], null, TextCompare: false) == 0)
					{
						StrCheckN[i] = "";
					}
					if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", TextCompare: false) != 0)
					{
						num = i * 12 + num2;
						e.Graphics.DrawString(StrCheckN[i], new System.Drawing.Font("Consolas", 8f), Brushes.Black, 0f, num);
					}
					else
					{
						if (!((TypWWW < 3) | (TypWWW == 8)))
						{
							continue;
						}
						num = i * 12 + 6 + num2;
						if (Dlstr == 29)
						{
							if (Operators.CompareString(text, "0", TextCompare: false) == 0)
							{
								e.Graphics.DrawImage(QrCode.Image, 36, num, 118, 118);
							}
							else if (Operators.CompareString(text, "1", TextCompare: false) == 0)
							{
								e.Graphics.DrawImage(QrCode.Image, 9, num, 172, 172);
							}
							else
							{
								e.Graphics.DrawImage(QrCode.Image, 36, num, 118, 118);
								All.f.WriteString("Global", "QrCode", "0");
							}
						}
						else if (Operators.CompareString(text, "0", TextCompare: false) == 0)
						{
							e.Graphics.DrawImage(QrCode.Image, 69, num, 118, 118);
						}
						else if (Operators.CompareString(text, "1", TextCompare: false) == 0)
						{
							e.Graphics.DrawImage(QrCode.Image, 45, num, 172, 172);
						}
						else
						{
							text = "0";
							e.Graphics.DrawImage(QrCode.Image, 69, num, 118, 118);
							All.f.WriteString("Global", "QrCode", text);
						}
						num2 = ((Operators.CompareString(text, "0", TextCompare: false) != 0) ? (num2 + 172) : (num2 + 118));
					}
				}
			}
			if (!All.A.FullVersion)
			{
				if ((Dlstr == 29) | (Dlstr == 39))
				{
					e.Graphics.DrawString("Безкоштовна версія ПРРО 'ВебЧек'", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 5f, num);
					num += 10;
					e.Graphics.DrawString("http://www.webchek.com.ua", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 18f, num);
				}
				else
				{
					e.Graphics.DrawString("Безкоштовна версія ПРРО 'ВебЧек'", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 35f, num);
					num += 10;
					e.Graphics.DrawString("http://www.webchek.com.ua", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 48f, num);
				}
			}
			else if ((Dlstr == 29) | (Dlstr == 39))
			{
				e.Graphics.DrawString("ПРРО  'ВебЧек'", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 54f, num);
			}
			else
			{
				e.Graphics.DrawString("ПРРО  'ВебЧек'", new System.Drawing.Font("Consolas", 7f), Brushes.Black, 93f, num);
			}
			if (All.A.ChekFooterSection > 0 && All.A.FullVersion && TypWWW == 0)
			{
				WordWord wordWord = new WordWord();
				wordWord.LL = Dlstr;
				string text2 = All.MyDoc() + "\\WebCheck\\Logo\\delimiter.jpg";
				System.Drawing.Image image = default(System.Drawing.Image);
				if (File.Exists(text2))
				{
					image = System.Drawing.Image.FromFile(text2);
				}
				try
				{
					num += 18;
					if ((Dlstr == 29) | (Dlstr == 39))
					{
						e.Graphics.DrawImage(image, 0, num, 183, 15);
					}
					else if (All.A.ecoPrint)
					{
						e.Graphics.DrawImage(image, 27, num, 183, 15);
					}
					else
					{
						e.Graphics.DrawImage(image, 36, num, 183, 15);
					}
				}
				catch (Exception ex3)
				{
					ProjectData.SetProjectError(ex3);
					Exception ex4 = ex3;
					num -= 18;
					ProjectData.ClearProjectError();
				}
				TypRndText typRndText = RandomText();
				string s = wordWord.CenterAlignment(typRndText.SectionName);
				if (All.A.ecoPrint)
				{
					num += 18;
					e.Graphics.DrawString(s, new System.Drawing.Font("Consolas", 6f), Brushes.Black, 0f, num);
					num2 = 12 + num;
				}
				else
				{
					num += 18;
					e.Graphics.DrawString(s, new System.Drawing.Font("Consolas", 8f), Brushes.Black, 0f, num);
					num2 = 18 + num;
				}
				wordWord.ParsingS(typRndText.RandomText);
				int num5 = wordWord.TextD.Length - 1;
				for (int i = 0; i <= num5; i++)
				{
					if (All.A.ecoPrint)
					{
						num = i * 8 + num2;
						e.Graphics.DrawString(wordWord.TextD[i], new System.Drawing.Font("Consolas", 6f), Brushes.Black, 0f, num);
					}
					else
					{
						num = i * 12 + num2;
						e.Graphics.DrawString(wordWord.TextD[i], new System.Drawing.Font("Consolas", 8f), Brushes.Black, 0f, num);
					}
				}
				try
				{
					num += 18;
					if ((Dlstr == 29) | (Dlstr == 39))
					{
						e.Graphics.DrawImage(image, 0, num, 183, 15);
					}
					else if (All.A.ecoPrint)
					{
						e.Graphics.DrawImage(image, 27, num, 183, 15);
					}
					else
					{
						e.Graphics.DrawImage(image, 36, num, 183, 15);
					}
				}
				catch (Exception ex5)
				{
					ProjectData.SetProjectError(ex5);
					Exception ex6 = ex5;
					num -= 27;
					ProjectData.ClearProjectError();
				}
			}
			num += 49;
			e.Graphics.DrawString(".", new System.Drawing.Font("Consolas", 7f), Brushes.Gray, 5f, num);
		}
	}

	private TypRndText RandomText()
	{
		TypRndText result = default(TypRndText);
		result.SectionName = "";
		result.RandomText = "";
		IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Logo\\ChekFooterSection.ini");
		string text = iniHGB.NameFn(All.A.ChekFooterSection);
		int num = 0;
		int num2 = 1;
		checked
		{
			do
			{
				if (iniHGB.StringGetFn(text, num2.ToString()).Length <= 3)
				{
					num = num2 - 1;
					break;
				}
				num2++;
			}
			while (num2 <= 999);
			if (num > 0)
			{
				VBMath.Randomize();
				num = (int)Conversion.Int((float)num * VBMath.Rnd() + 1f);
				string randomText = iniHGB.StringGetFn(text, Conversions.ToString(num));
				result.SectionName = text;
				result.RandomText = randomText;
			}
			iniHGB = null;
			return result;
		}
	}

	private void ВибірПринтераToolStripMenuItem_Click(object sender, EventArgs e)
	{
		base.TopMost = false;
		if (PrintDialog1.ShowDialog() == DialogResult.OK)
		{
			All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
			PrintDocument1.PrinterSettings = PrintDialog1.PrinterSettings;
			PrintDocument1.Print();
			All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
		}
	}

	private void НалаштуванняДрукуToolStripMenuItem_Click(object sender, EventArgs e)
	{
		base.TopMost = false;
		PrintPreviewDialog1.Document = PrintDocument1;
		if (PrintPreviewDialog1.ShowDialog() == DialogResult.OK)
		{
			PrintDocument1.Print();
		}
	}

	private void ДрукToolStripMenuItem_Click(object sender, EventArgs e)
	{
		base.TopMost = false;
		PrintDocument1.Print();
	}

	private void ОстаннійЧекToolStripMenuItem_Click(object sender, EventArgs e)
	{
		nPrint = "";
		Zapolnili = false;
		ResCheck();
	}

	private void ОстаннійZЗвітToolStripMenuItem_Click(object sender, EventArgs e)
	{
		nPrint = "z";
		Zapolnili = false;
		ResCheck();
	}

	private int TypChekcs(string xmlCheck)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(xmlCheck, "t", "rq/dat/c");
		if (parametrToString.errCode == 0)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, "0", TextCompare: false) == 0)
			{
				return 0;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "1", TextCompare: false) == 0)
			{
				return 1;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "2", TextCompare: false) == 0)
			{
				return 2;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "8", TextCompare: false) == 0)
			{
				return -8;
			}
		}
		if (All.d.GetParametrToString(xmlCheck, "no", "rq/dat/z").errCode == 0)
		{
			return 3;
		}
		return -1;
	}

	private void ExportToPdf()
	{
		checked
		{
			try
			{
				Document document = new Document();
				string name = Environment.GetFolderPath(Environment.SpecialFolder.Fonts) + "\\consola.ttf";
				string path = All.MyDoc() + "\\WebCheck\\Temp\\" + TB1.Text + ".pdf";
				if (File.Exists(path))
				{
					Interaction.MsgBox("Такий файл вже є", MsgBoxStyle.Information, "Експорт в PDF");
					return;
				}
				BaseFont bf = BaseFont.CreateFont(name, "CP1251", embedded: true);
				iTextSharp.text.Font font = new iTextSharp.text.Font(bf, 18f, 0);
				PdfWriter.GetInstance(document, new FileStream(path, FileMode.Create));
				document.Open();
				int num = StrCheckN.Count() - 1;
				for (int i = 0; i <= num; i++)
				{
					if (Operators.CompareString(StrCheckN[i].Trim(), "HotGamesBest", TextCompare: false) != 0)
					{
						document.Add(new Paragraph(StrCheckN[i], font));
					}
				}
				document.Close();
				Interaction.MsgBox("Чек збережений у форматі PDF", MsgBoxStyle.Information, "Експорт в PDF");
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				Interaction.MsgBox("Помилка експорту чека!", MsgBoxStyle.Exclamation, "Експорт в PDF");
				ProjectData.ClearProjectError();
			}
		}
	}

	private void ЕкспортВToolStripMenuItem_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(TB1.Text, "", TextCompare: false) != 0)
		{
			ExportToPdf();
		}
	}

	private void QrCode_DoubleClick(object sender, EventArgs e)
	{
		OpenURL(QrCode.Text);
	}

	public void OpenURL(string wwwURL)
	{
		try
		{
			Process.Start(wwwURL);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void LinkCopy_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}

	private void ВсіЗміниToolStripMenuItem_Click(object sender, EventArgs e)
	{
		base.TopMost = false;
		new FormReports().ShowDialog();
	}

	private void EndB_Click(object sender, EventArgs e)
	{
		Close();
	}

	private void SmsB_Click(object sender, EventArgs e)
	{
		if (Operators.CompareString(TB2.Text.Trim(), "", TextCompare: false) != 0)
		{
			FormViber formViber = new FormViber(TB2.Text.Trim());
			formViber.ShowDialog();
			formViber.Dispose();
		}
	}

	private void ЛінкВБуферОбмінуToolStripMenuItem_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}

	private void QrCode_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}

	private void CheckEco_CheckedChanged(object sender, EventArgs e)
	{
		if (CheckEco.Checked == All.A.ecoPrint)
		{
			return;
		}
		if (CheckEco.Checked && !All.A.FullVersion)
		{
			CheckEco.Checked = false;
			All.A.ecoPrint = false;
			All.f.StringWriteFN(All.A.FN, "EcoPrt", "0");
			Interaction.MsgBox("Економний друк доступний лише у платній версії!", MsgBoxStyle.OkOnly, "Друк чеків");
			return;
		}
		All.A.ecoPrint = CheckEco.Checked;
		if (Rb1.Checked)
		{
			if (All.A.ecoPrint & All.A.FullVersion)
			{
				Dlstr = 39;
			}
			else
			{
				Dlstr = 29;
			}
		}
		else if (Rb2.Checked)
		{
			if (All.A.ecoPrint & All.A.FullVersion)
			{
				Dlstr = 50;
			}
			else
			{
				Dlstr = 40;
			}
		}
		if (All.A.ecoPrint)
		{
			All.f.StringWriteFN(All.A.FN, "EcoPrt", "1");
		}
		else
		{
			All.f.StringWriteFN(All.A.FN, "EcoPrt", "0");
		}
		ResCheck();
	}
}
