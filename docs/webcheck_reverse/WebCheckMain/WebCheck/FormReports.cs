using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Windows.Forms;
using Gma.QrCodeNet.Encoding;
using Gma.QrCodeNet.Encoding.Windows.Forms;
using Gma.QrCodeNet.Encoding.Windows.Render;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

[DesignerGenerated]
internal class FormReports : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("DG")]
	private DataGridView _DG;

	[CompilerGenerated]
	[AccessedThroughProperty("QrCode")]
	private QrCodeImgControl _QrCode;

	[CompilerGenerated]
	[AccessedThroughProperty("DG1")]
	private DataGridView _DG1;

	[CompilerGenerated]
	[AccessedThroughProperty("RetSheft")]
	private Button _RetSheft;

	[CompilerGenerated]
	[AccessedThroughProperty("Druk")]
	private Button _Druk;

	[CompilerGenerated]
	[AccessedThroughProperty("Button1")]
	private Button _Button1;

	[CompilerGenerated]
	[AccessedThroughProperty("Zakrit")]
	private Button _Zakrit;

	[CompilerGenerated]
	[AccessedThroughProperty("Acb")]
	private CheckBox _Acb;

	[CompilerGenerated]
	[AccessedThroughProperty("Mcb")]
	private ComboBox _Mcb;

	[CompilerGenerated]
	[AccessedThroughProperty("Ycb")]
	private ComboBox _Ycb;

	[CompilerGenerated]
	[AccessedThroughProperty("VibB")]
	private Button _VibB;

	private bool VidShift;

	private int LocalNumerCheck;

	private Image PrintLogo;

	internal virtual DataGridView DG
	{
		[CompilerGenerated]
		get
		{
			return _DG;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			DataGridViewCellEventHandler val = new DataGridViewCellEventHandler(DG_CellDoubleClick);
			DataGridView dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick -= val;
			}
			_DG = value;
			dG = _DG;
			if (dG != null)
			{
				dG.CellDoubleClick += val;
			}
		}
	}

	[field: AccessedThroughProperty("Tb")]
	internal virtual TextBox Tb
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
			EventHandler eventHandler = QrCode_DoubleClick;
			EventHandler eventHandler2 = QrCode_Click;
			QrCodeImgControl qrCode = _QrCode;
			if (qrCode != null)
			{
				((Control)qrCode).DoubleClick -= eventHandler;
				((Control)qrCode).Click -= eventHandler2;
			}
			_QrCode = value;
			qrCode = _QrCode;
			if (qrCode != null)
			{
				((Control)qrCode).DoubleClick += eventHandler;
				((Control)qrCode).Click += eventHandler2;
			}
		}
	}

	internal virtual DataGridView DG1
	{
		[CompilerGenerated]
		get
		{
			return _DG1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			//IL_0007: Unknown result type (might be due to invalid IL or missing references)
			//IL_000d: Expected O, but got Unknown
			DataGridViewCellEventHandler val = new DataGridViewCellEventHandler(DG1_CellDoubleClick);
			DataGridView dG = _DG1;
			if (dG != null)
			{
				dG.CellDoubleClick -= val;
			}
			_DG1 = value;
			dG = _DG1;
			if (dG != null)
			{
				dG.CellDoubleClick += val;
			}
		}
	}

	internal virtual Button RetSheft
	{
		[CompilerGenerated]
		get
		{
			return _RetSheft;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = RetSheft_Click;
			Button retSheft = _RetSheft;
			if (retSheft != null)
			{
				((Control)retSheft).Click -= eventHandler;
			}
			_RetSheft = value;
			retSheft = _RetSheft;
			if (retSheft != null)
			{
				((Control)retSheft).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("GB")]
	internal virtual GroupBox GB
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("GB1")]
	internal virtual GroupBox GB1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("TB2")]
	internal virtual TextBox TB2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T1")]
	internal virtual TextBox T1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T2")]
	internal virtual TextBox T2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T3")]
	internal virtual TextBox T3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("T4")]
	internal virtual TextBox T4
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
			EventHandler eventHandler = Druk_Click;
			Button druk = _Druk;
			if (druk != null)
			{
				((Control)druk).Click -= eventHandler;
			}
			_Druk = value;
			druk = _Druk;
			if (druk != null)
			{
				((Control)druk).Click += eventHandler;
			}
		}
	}

	internal virtual Button Button1
	{
		[CompilerGenerated]
		get
		{
			return _Button1;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Button1_Click;
			Button button = _Button1;
			if (button != null)
			{
				((Control)button).Click -= eventHandler;
			}
			_Button1 = value;
			button = _Button1;
			if (button != null)
			{
				((Control)button).Click += eventHandler;
			}
		}
	}

	internal virtual Button Zakrit
	{
		[CompilerGenerated]
		get
		{
			return _Zakrit;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Zakrit_Click;
			Button zakrit = _Zakrit;
			if (zakrit != null)
			{
				((Control)zakrit).Click -= eventHandler;
			}
			_Zakrit = value;
			zakrit = _Zakrit;
			if (zakrit != null)
			{
				((Control)zakrit).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("PrintDialog1")]
	internal virtual PrintDialog PrintDialog1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn1")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn2")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn3")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn4")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("DataGridViewTextBoxColumn5")]
	internal virtual DataGridViewTextBoxColumn DataGridViewTextBoxColumn5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column6")]
	internal virtual DataGridViewTextBoxColumn Column6
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column1")]
	internal virtual DataGridViewTextBoxColumn Column1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column2")]
	internal virtual DataGridViewTextBoxColumn Column2
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column3")]
	internal virtual DataGridViewTextBoxColumn Column3
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column4")]
	internal virtual DataGridViewTextBoxColumn Column4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Column5")]
	internal virtual DataGridViewTextBoxColumn Column5
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual CheckBox Acb
	{
		[CompilerGenerated]
		get
		{
			return _Acb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Acb_CheckedChanged;
			CheckBox acb = _Acb;
			if (acb != null)
			{
				acb.CheckedChanged -= eventHandler;
			}
			_Acb = value;
			acb = _Acb;
			if (acb != null)
			{
				acb.CheckedChanged += eventHandler;
			}
		}
	}

	internal virtual ComboBox Mcb
	{
		[CompilerGenerated]
		get
		{
			return _Mcb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Mcb_SelectedIndexChanged;
			ComboBox mcb = _Mcb;
			if (mcb != null)
			{
				mcb.SelectedIndexChanged -= eventHandler;
			}
			_Mcb = value;
			mcb = _Mcb;
			if (mcb != null)
			{
				mcb.SelectedIndexChanged += eventHandler;
			}
		}
	}

	internal virtual ComboBox Ycb
	{
		[CompilerGenerated]
		get
		{
			return _Ycb;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = Ycb_SelectedIndexChanged;
			ComboBox ycb = _Ycb;
			if (ycb != null)
			{
				ycb.SelectedIndexChanged -= eventHandler;
			}
			_Ycb = value;
			ycb = _Ycb;
			if (ycb != null)
			{
				ycb.SelectedIndexChanged += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("Label4")]
	internal virtual Label Label4
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("Label3")]
	internal virtual Label Label3
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

	[field: AccessedThroughProperty("Label1")]
	internal virtual Label Label1
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

	internal virtual Button VibB
	{
		[CompilerGenerated]
		get
		{
			return _VibB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = VibB_Click;
			Button vibB = _VibB;
			if (vibB != null)
			{
				((Control)vibB).Click -= eventHandler;
			}
			_VibB = value;
			vibB = _VibB;
			if (vibB != null)
			{
				((Control)vibB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolTip1")]
	internal virtual ToolTip ToolTip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	public FormReports()
	{
		((Form)this).Load += FormReports_Load;
		VidShift = true;
		LocalNumerCheck = 0;
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
		//IL_000b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0011: Expected O, but got Unknown
		//IL_0021: Unknown result type (might be due to invalid IL or missing references)
		//IL_0027: Expected O, but got Unknown
		//IL_0028: Unknown result type (might be due to invalid IL or missing references)
		//IL_0032: Expected O, but got Unknown
		//IL_0033: Unknown result type (might be due to invalid IL or missing references)
		//IL_003d: Expected O, but got Unknown
		//IL_003e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0048: Expected O, but got Unknown
		//IL_0049: Unknown result type (might be due to invalid IL or missing references)
		//IL_0053: Expected O, but got Unknown
		//IL_0054: Unknown result type (might be due to invalid IL or missing references)
		//IL_005e: Expected O, but got Unknown
		//IL_005f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0069: Expected O, but got Unknown
		//IL_006a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0074: Expected O, but got Unknown
		//IL_0080: Unknown result type (might be due to invalid IL or missing references)
		//IL_008a: Expected O, but got Unknown
		//IL_008b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0095: Expected O, but got Unknown
		//IL_0096: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a0: Expected O, but got Unknown
		//IL_00a1: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ab: Expected O, but got Unknown
		//IL_00ac: Unknown result type (might be due to invalid IL or missing references)
		//IL_00b6: Expected O, but got Unknown
		//IL_00b7: Unknown result type (might be due to invalid IL or missing references)
		//IL_00c1: Expected O, but got Unknown
		//IL_00c2: Unknown result type (might be due to invalid IL or missing references)
		//IL_00cc: Expected O, but got Unknown
		//IL_00cd: Unknown result type (might be due to invalid IL or missing references)
		//IL_00d7: Expected O, but got Unknown
		//IL_00d8: Unknown result type (might be due to invalid IL or missing references)
		//IL_00e2: Expected O, but got Unknown
		//IL_00e3: Unknown result type (might be due to invalid IL or missing references)
		//IL_00ed: Expected O, but got Unknown
		//IL_00ee: Unknown result type (might be due to invalid IL or missing references)
		//IL_00f8: Expected O, but got Unknown
		//IL_00f9: Unknown result type (might be due to invalid IL or missing references)
		//IL_0103: Expected O, but got Unknown
		//IL_0104: Unknown result type (might be due to invalid IL or missing references)
		//IL_010e: Expected O, but got Unknown
		//IL_010f: Unknown result type (might be due to invalid IL or missing references)
		//IL_0119: Expected O, but got Unknown
		//IL_011a: Unknown result type (might be due to invalid IL or missing references)
		//IL_0124: Expected O, but got Unknown
		//IL_0125: Unknown result type (might be due to invalid IL or missing references)
		//IL_012f: Expected O, but got Unknown
		//IL_0130: Unknown result type (might be due to invalid IL or missing references)
		//IL_013a: Expected O, but got Unknown
		//IL_013b: Unknown result type (might be due to invalid IL or missing references)
		//IL_0145: Expected O, but got Unknown
		//IL_0146: Unknown result type (might be due to invalid IL or missing references)
		//IL_0150: Expected O, but got Unknown
		//IL_0151: Unknown result type (might be due to invalid IL or missing references)
		//IL_015b: Expected O, but got Unknown
		//IL_015c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0166: Expected O, but got Unknown
		//IL_0167: Unknown result type (might be due to invalid IL or missing references)
		//IL_0171: Expected O, but got Unknown
		//IL_0172: Unknown result type (might be due to invalid IL or missing references)
		//IL_017c: Expected O, but got Unknown
		//IL_017d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0187: Expected O, but got Unknown
		//IL_0188: Unknown result type (might be due to invalid IL or missing references)
		//IL_0192: Expected O, but got Unknown
		//IL_0193: Unknown result type (might be due to invalid IL or missing references)
		//IL_019d: Expected O, but got Unknown
		//IL_019e: Unknown result type (might be due to invalid IL or missing references)
		//IL_01a8: Expected O, but got Unknown
		//IL_01a9: Unknown result type (might be due to invalid IL or missing references)
		//IL_01b3: Expected O, but got Unknown
		//IL_01ba: Unknown result type (might be due to invalid IL or missing references)
		//IL_01c4: Expected O, but got Unknown
		//IL_04b0: Unknown result type (might be due to invalid IL or missing references)
		//IL_04ba: Expected O, but got Unknown
		//IL_04db: Unknown result type (might be due to invalid IL or missing references)
		//IL_0574: Unknown result type (might be due to invalid IL or missing references)
		//IL_057e: Expected O, but got Unknown
		//IL_08f3: Unknown result type (might be due to invalid IL or missing references)
		//IL_08fd: Expected O, but got Unknown
		//IL_0a95: Unknown result type (might be due to invalid IL or missing references)
		//IL_0a9f: Expected O, but got Unknown
		//IL_0d15: Unknown result type (might be due to invalid IL or missing references)
		//IL_0d1f: Expected O, but got Unknown
		//IL_0da9: Unknown result type (might be due to invalid IL or missing references)
		//IL_0db3: Expected O, but got Unknown
		//IL_0e36: Unknown result type (might be due to invalid IL or missing references)
		//IL_0e40: Expected O, but got Unknown
		//IL_0eba: Unknown result type (might be due to invalid IL or missing references)
		//IL_0ec4: Expected O, but got Unknown
		//IL_0f3e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0f48: Expected O, but got Unknown
		//IL_0fbf: Unknown result type (might be due to invalid IL or missing references)
		//IL_0fc9: Expected O, but got Unknown
		//IL_1043: Unknown result type (might be due to invalid IL or missing references)
		//IL_104d: Expected O, but got Unknown
		//IL_113b: Unknown result type (might be due to invalid IL or missing references)
		//IL_1145: Expected O, but got Unknown
		//IL_11d3: Unknown result type (might be due to invalid IL or missing references)
		//IL_11dd: Expected O, but got Unknown
		//IL_11fe: Unknown result type (might be due to invalid IL or missing references)
		//IL_1292: Unknown result type (might be due to invalid IL or missing references)
		//IL_129c: Expected O, but got Unknown
		//IL_12c0: Unknown result type (might be due to invalid IL or missing references)
		//IL_1354: Unknown result type (might be due to invalid IL or missing references)
		//IL_135e: Expected O, but got Unknown
		//IL_137f: Unknown result type (might be due to invalid IL or missing references)
		//IL_1410: Unknown result type (might be due to invalid IL or missing references)
		//IL_141a: Expected O, but got Unknown
		//IL_143e: Unknown result type (might be due to invalid IL or missing references)
		//IL_14d0: Unknown result type (might be due to invalid IL or missing references)
		//IL_14da: Expected O, but got Unknown
		//IL_15df: Unknown result type (might be due to invalid IL or missing references)
		//IL_15e9: Expected O, but got Unknown
		components = new Container();
		DataGridViewCellStyle val = new DataGridViewCellStyle();
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormReports));
		DataGridViewCellStyle val2 = new DataGridViewCellStyle();
		DG = new DataGridView();
		Column1 = new DataGridViewTextBoxColumn();
		Column2 = new DataGridViewTextBoxColumn();
		Column3 = new DataGridViewTextBoxColumn();
		Column4 = new DataGridViewTextBoxColumn();
		Column5 = new DataGridViewTextBoxColumn();
		Tb = new TextBox();
		QrCode = new QrCodeImgControl();
		DG1 = new DataGridView();
		DataGridViewTextBoxColumn1 = new DataGridViewTextBoxColumn();
		DataGridViewTextBoxColumn2 = new DataGridViewTextBoxColumn();
		DataGridViewTextBoxColumn3 = new DataGridViewTextBoxColumn();
		DataGridViewTextBoxColumn4 = new DataGridViewTextBoxColumn();
		DataGridViewTextBoxColumn5 = new DataGridViewTextBoxColumn();
		Column6 = new DataGridViewTextBoxColumn();
		RetSheft = new Button();
		GB = new GroupBox();
		Label5 = new Label();
		Label4 = new Label();
		Label3 = new Label();
		Label2 = new Label();
		Label1 = new Label();
		Acb = new CheckBox();
		T4 = new TextBox();
		Ycb = new ComboBox();
		T2 = new TextBox();
		T3 = new TextBox();
		Mcb = new ComboBox();
		T1 = new TextBox();
		GB1 = new GroupBox();
		VibB = new Button();
		Zakrit = new Button();
		Button1 = new Button();
		Druk = new Button();
		TB2 = new TextBox();
		PrintDialog1 = new PrintDialog();
		ToolTip1 = new ToolTip(components);
		((ISupportInitialize)DG).BeginInit();
		((ISupportInitialize)QrCode).BeginInit();
		((ISupportInitialize)DG1).BeginInit();
		((Control)GB).SuspendLayout();
		((Control)GB1).SuspendLayout();
		((Control)this).SuspendLayout();
		DG.AllowUserToAddRows = false;
		DG.AllowUserToDeleteRows = false;
		((Control)DG).Anchor = (AnchorStyles)15;
		DG.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[5]
		{
			(DataGridViewColumn)Column1,
			(DataGridViewColumn)Column2,
			(DataGridViewColumn)Column3,
			(DataGridViewColumn)Column4,
			(DataGridViewColumn)Column5
		});
		((Control)DG).Location = new Point(624, 12);
		DG.MultiSelect = false;
		((Control)DG).Name = "DG";
		DG.ReadOnly = true;
		DG.RowHeadersWidth = 51;
		DG.RowTemplate.Height = 24;
		((Control)DG).Size = new Size(701, 587);
		((Control)DG).TabIndex = 0;
		val.NullValue = null;
		((DataGridViewColumn)Column1).DefaultCellStyle = val;
		((DataGridViewColumn)Column1).HeaderText = "№";
		((DataGridViewColumn)Column1).MinimumWidth = 6;
		((DataGridViewColumn)Column1).Name = "Column1";
		((DataGridViewColumn)Column1).ReadOnly = true;
		((DataGridViewColumn)Column1).Width = 50;
		((DataGridViewColumn)Column2).HeaderText = "Відкрита";
		((DataGridViewColumn)Column2).MinimumWidth = 6;
		((DataGridViewColumn)Column2).Name = "Column2";
		((DataGridViewColumn)Column2).ReadOnly = true;
		((DataGridViewColumn)Column2).Width = 125;
		((DataGridViewColumn)Column3).HeaderText = "Закрита";
		((DataGridViewColumn)Column3).MinimumWidth = 6;
		((DataGridViewColumn)Column3).Name = "Column3";
		((DataGridViewColumn)Column3).ReadOnly = true;
		((DataGridViewColumn)Column3).Width = 125;
		((DataGridViewColumn)Column4).HeaderText = "Оператор";
		((DataGridViewColumn)Column4).MinimumWidth = 6;
		((DataGridViewColumn)Column4).Name = "Column4";
		((DataGridViewColumn)Column4).ReadOnly = true;
		((DataGridViewColumn)Column4).Width = 125;
		((DataGridViewColumn)Column5).HeaderText = "Чеків";
		((DataGridViewColumn)Column5).MinimumWidth = 6;
		((DataGridViewColumn)Column5).Name = "Column5";
		((DataGridViewColumn)Column5).ReadOnly = true;
		Column5.SortMode = (DataGridViewColumnSortMode)0;
		((DataGridViewColumn)Column5).Width = 50;
		((Control)Tb).Anchor = (AnchorStyles)7;
		((TextBoxBase)Tb).BackColor = Color.White;
		((TextBoxBase)Tb).BorderStyle = (BorderStyle)0;
		((Control)Tb).Font = new Font("Consolas", 9f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Tb).Location = new Point(12, 245);
		((Control)Tb).Margin = new Padding(3, 2, 3, 2);
		Tb.Multiline = true;
		((Control)Tb).Name = "Tb";
		((TextBoxBase)Tb).ReadOnly = true;
		Tb.ScrollBars = (ScrollBars)2;
		((Control)Tb).Size = new Size(606, 354);
		((Control)Tb).TabIndex = 2;
		((Control)Tb).TabStop = false;
		Tb.TextAlign = (HorizontalAlignment)2;
		QrCode.ErrorCorrectLevel = ErrorCorrectionLevel.M;
		((PictureBox)QrCode).Image = (Image)componentResourceManager.GetObject("QrCode.Image");
		((Control)QrCode).Location = new Point(11, 28);
		((Control)QrCode).Name = "QrCode";
		QrCode.QuietZoneModule = QuietZoneModules.Two;
		((Control)QrCode).Size = new Size(198, 198);
		((PictureBox)QrCode).SizeMode = (PictureBoxSizeMode)4;
		((PictureBox)QrCode).TabIndex = 6;
		((PictureBox)QrCode).TabStop = false;
		QrCode.Text = "WebCheck";
		ToolTip1.SetToolTip((Control)(object)QrCode, "QR код чека");
		DG1.AllowUserToAddRows = false;
		DG1.AllowUserToDeleteRows = false;
		((Control)DG1).Anchor = (AnchorStyles)15;
		DG1.ColumnHeadersHeightSizeMode = (DataGridViewColumnHeadersHeightSizeMode)2;
		DG1.Columns.AddRange((DataGridViewColumn[])(object)new DataGridViewColumn[6]
		{
			(DataGridViewColumn)DataGridViewTextBoxColumn1,
			(DataGridViewColumn)DataGridViewTextBoxColumn2,
			(DataGridViewColumn)DataGridViewTextBoxColumn3,
			(DataGridViewColumn)DataGridViewTextBoxColumn4,
			(DataGridViewColumn)DataGridViewTextBoxColumn5,
			(DataGridViewColumn)Column6
		});
		((Control)DG1).Location = new Point(652, 56);
		DG1.MultiSelect = false;
		((Control)DG1).Name = "DG1";
		DG1.ReadOnly = true;
		DG1.RowHeadersWidth = 51;
		DG1.RowTemplate.Height = 24;
		((Control)DG1).Size = new Size(701, 587);
		((Control)DG1).TabIndex = 7;
		val2.NullValue = null;
		((DataGridViewColumn)DataGridViewTextBoxColumn1).DefaultCellStyle = val2;
		((DataGridViewColumn)DataGridViewTextBoxColumn1).HeaderText = "№";
		((DataGridViewColumn)DataGridViewTextBoxColumn1).MinimumWidth = 6;
		((DataGridViewColumn)DataGridViewTextBoxColumn1).Name = "DataGridViewTextBoxColumn1";
		((DataGridViewColumn)DataGridViewTextBoxColumn1).ReadOnly = true;
		((DataGridViewColumn)DataGridViewTextBoxColumn1).Width = 50;
		((DataGridViewColumn)DataGridViewTextBoxColumn2).HeaderText = "Ідентифікатор";
		((DataGridViewColumn)DataGridViewTextBoxColumn2).MinimumWidth = 6;
		((DataGridViewColumn)DataGridViewTextBoxColumn2).Name = "DataGridViewTextBoxColumn2";
		((DataGridViewColumn)DataGridViewTextBoxColumn2).ReadOnly = true;
		DataGridViewTextBoxColumn2.SortMode = (DataGridViewColumnSortMode)0;
		((DataGridViewColumn)DataGridViewTextBoxColumn2).Width = 108;
		((DataGridViewColumn)DataGridViewTextBoxColumn3).HeaderText = "Тип";
		((DataGridViewColumn)DataGridViewTextBoxColumn3).MinimumWidth = 6;
		((DataGridViewColumn)DataGridViewTextBoxColumn3).Name = "DataGridViewTextBoxColumn3";
		((DataGridViewColumn)DataGridViewTextBoxColumn3).ReadOnly = true;
		((DataGridViewColumn)DataGridViewTextBoxColumn3).Width = 75;
		((DataGridViewColumn)DataGridViewTextBoxColumn4).HeaderText = "Дата";
		((DataGridViewColumn)DataGridViewTextBoxColumn4).MinimumWidth = 6;
		((DataGridViewColumn)DataGridViewTextBoxColumn4).Name = "DataGridViewTextBoxColumn4";
		((DataGridViewColumn)DataGridViewTextBoxColumn4).ReadOnly = true;
		((DataGridViewColumn)DataGridViewTextBoxColumn4).Width = 127;
		((DataGridViewColumn)DataGridViewTextBoxColumn5).HeaderText = "Сума";
		((DataGridViewColumn)DataGridViewTextBoxColumn5).MinimumWidth = 6;
		((DataGridViewColumn)DataGridViewTextBoxColumn5).Name = "DataGridViewTextBoxColumn5";
		((DataGridViewColumn)DataGridViewTextBoxColumn5).ReadOnly = true;
		((DataGridViewColumn)DataGridViewTextBoxColumn5).Width = 125;
		((DataGridViewColumn)Column6).HeaderText = "ID";
		((DataGridViewColumn)Column6).MinimumWidth = 6;
		((DataGridViewColumn)Column6).Name = "Column6";
		((DataGridViewColumn)Column6).ReadOnly = true;
		((DataGridViewColumn)Column6).Visible = false;
		((DataGridViewColumn)Column6).Width = 125;
		((Control)RetSheft).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)1, (GraphicsUnit)3, (byte)204);
		((Control)RetSheft).Location = new Point(558, 12);
		((Control)RetSheft).Name = "RetSheft";
		((Control)RetSheft).Size = new Size(60, 40);
		((Control)RetSheft).TabIndex = 8;
		((ButtonBase)RetSheft).Text = "<<";
		ToolTip1.SetToolTip((Control)(object)RetSheft, "Повернися до списку змін ");
		((ButtonBase)RetSheft).UseVisualStyleBackColor = true;
		((Control)GB).Controls.Add((Control)(object)Label5);
		((Control)GB).Controls.Add((Control)(object)Label4);
		((Control)GB).Controls.Add((Control)(object)Label3);
		((Control)GB).Controls.Add((Control)(object)Label2);
		((Control)GB).Controls.Add((Control)(object)Label1);
		((Control)GB).Controls.Add((Control)(object)Acb);
		((Control)GB).Controls.Add((Control)(object)T4);
		((Control)GB).Controls.Add((Control)(object)Ycb);
		((Control)GB).Controls.Add((Control)(object)T2);
		((Control)GB).Controls.Add((Control)(object)T3);
		((Control)GB).Controls.Add((Control)(object)Mcb);
		((Control)GB).Controls.Add((Control)(object)T1);
		((Control)GB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GB).Location = new Point(12, 4);
		((Control)GB).Name = "GB";
		((Control)GB).Size = new Size(540, 236);
		((Control)GB).TabIndex = 9;
		GB.TabStop = false;
		GB.Text = "ФН";
		Label5.AutoSize = true;
		((Control)Label5).Location = new Point(6, 192);
		((Control)Label5).Name = "Label5";
		((Control)Label5).Size = new Size(101, 25);
		((Control)Label5).TabIndex = 19;
		Label5.Text = "Показати";
		Label4.AutoSize = true;
		((Control)Label4).Location = new Point(28, 131);
		((Control)Label4).Name = "Label4";
		((Control)Label4).Size = new Size(79, 25);
		((Control)Label4).TabIndex = 18;
		Label4.Text = "Адреса";
		Label3.AutoSize = true;
		((Control)Label3).Location = new Point(41, 75);
		((Control)Label3).Name = "Label3";
		((Control)Label3).Size = new Size(66, 25);
		((Control)Label3).TabIndex = 17;
		Label3.Text = "Назва";
		Label2.AutoSize = true;
		((Control)Label2).Location = new Point(303, 35);
		((Control)Label2).Name = "Label2";
		((Control)Label2).Size = new Size(44, 25);
		((Control)Label2).TabIndex = 16;
		Label2.Text = "TIN";
		Label1.AutoSize = true;
		((Control)Label1).Location = new Point(62, 35);
		((Control)Label1).Name = "Label1";
		((Control)Label1).Size = new Size(45, 25);
		((Control)Label1).TabIndex = 15;
		Label1.Text = "INN";
		((ButtonBase)Acb).AutoSize = true;
		((Control)Acb).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Acb).Location = new Point(463, 191);
		((Control)Acb).Name = "Acb";
		((Control)Acb).Size = new Size(60, 29);
		((Control)Acb).TabIndex = 12;
		((ButtonBase)Acb).Text = "Всі";
		((ButtonBase)Acb).UseVisualStyleBackColor = true;
		((Control)T4).Enabled = false;
		((Control)T4).Font = new Font("Microsoft Sans Serif", 13.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)T4).Location = new Point(125, 108);
		T4.Multiline = true;
		((Control)T4).Name = "T4";
		((Control)T4).Size = new Size(398, 63);
		((Control)T4).TabIndex = 11;
		T4.TextAlign = (HorizontalAlignment)2;
		Ycb.DropDownStyle = (ComboBoxStyle)2;
		((Control)Ycb).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)Ycb).FormattingEnabled = true;
		((Control)Ycb).Location = new Point(344, 193);
		((Control)Ycb).Name = "Ycb";
		((Control)Ycb).Size = new Size(99, 28);
		((Control)Ycb).TabIndex = 14;
		((Control)T2).Enabled = false;
		((Control)T2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)T2).Location = new Point(356, 32);
		((Control)T2).Name = "T2";
		((Control)T2).Size = new Size(167, 30);
		((Control)T2).TabIndex = 10;
		T2.TextAlign = (HorizontalAlignment)2;
		((Control)T3).Enabled = false;
		((Control)T3).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)T3).Location = new Point(125, 72);
		((Control)T3).Name = "T3";
		((Control)T3).Size = new Size(398, 30);
		((Control)T3).TabIndex = 9;
		T3.TextAlign = (HorizontalAlignment)2;
		Mcb.DropDownStyle = (ComboBoxStyle)2;
		((Control)Mcb).Font = new Font("Microsoft Sans Serif", 10.2f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((ListControl)Mcb).FormattingEnabled = true;
		((Control)Mcb).Location = new Point(125, 191);
		((Control)Mcb).Name = "Mcb";
		((Control)Mcb).Size = new Size(163, 28);
		((Control)Mcb).TabIndex = 13;
		((Control)T1).Enabled = false;
		((Control)T1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)T1).Location = new Point(125, 32);
		((Control)T1).Name = "T1";
		((Control)T1).Size = new Size(167, 30);
		((Control)T1).TabIndex = 8;
		T1.TextAlign = (HorizontalAlignment)2;
		((Control)GB1).Controls.Add((Control)(object)VibB);
		((Control)GB1).Controls.Add((Control)(object)Zakrit);
		((Control)GB1).Controls.Add((Control)(object)Button1);
		((Control)GB1).Controls.Add((Control)(object)Druk);
		((Control)GB1).Controls.Add((Control)(object)TB2);
		((Control)GB1).Controls.Add((Control)(object)QrCode);
		((Control)GB1).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)GB1).Location = new Point(12, 267);
		((Control)GB1).Name = "GB1";
		((Control)GB1).Size = new Size(540, 236);
		((Control)GB1).TabIndex = 10;
		GB1.TabStop = false;
		GB1.Text = "Чек";
		((Control)VibB).Anchor = (AnchorStyles)9;
		((Control)VibB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)VibB).Location = new Point(222, 109);
		((Control)VibB).Margin = new Padding(3, 2, 3, 2);
		((Control)VibB).Name = "VibB";
		((Control)VibB).Size = new Size(308, 46);
		((Control)VibB).TabIndex = 11;
		((ButtonBase)VibB).Text = "VIBER/SMS...";
		ToolTip1.SetToolTip((Control)(object)VibB, "Надіслати чек покупцю на Вайбер або СМС ");
		((ButtonBase)VibB).UseVisualStyleBackColor = true;
		((Control)Zakrit).Anchor = (AnchorStyles)9;
		((Control)Zakrit).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Zakrit).Location = new Point(222, 171);
		((Control)Zakrit).Margin = new Padding(3, 2, 3, 2);
		((Control)Zakrit).Name = "Zakrit";
		((Control)Zakrit).Size = new Size(151, 46);
		((Control)Zakrit).TabIndex = 10;
		((ButtonBase)Zakrit).Text = "Принтер...";
		ToolTip1.SetToolTip((Control)(object)Zakrit, "Вибір принтера для друку ");
		((ButtonBase)Zakrit).UseVisualStyleBackColor = true;
		((Control)Button1).Anchor = (AnchorStyles)9;
		((Control)Button1).Font = new Font("Microsoft Sans Serif", 10.8f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Button1).Location = new Point(494, 29);
		((Control)Button1).Margin = new Padding(3, 2, 3, 2);
		((Control)Button1).Name = "Button1";
		((Control)Button1).Size = new Size(36, 34);
		((Control)Button1).TabIndex = 9;
		((ButtonBase)Button1).Text = "C";
		ToolTip1.SetToolTip((Control)(object)Button1, "Копіювати лінк на чек у буфер обміну");
		((ButtonBase)Button1).UseVisualStyleBackColor = true;
		((Control)Druk).Anchor = (AnchorStyles)9;
		((Control)Druk).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)Druk).Location = new Point(379, 171);
		((Control)Druk).Margin = new Padding(3, 2, 3, 2);
		((Control)Druk).Name = "Druk";
		((Control)Druk).Size = new Size(151, 46);
		((Control)Druk).TabIndex = 8;
		((ButtonBase)Druk).Text = "ДРУК";
		ToolTip1.SetToolTip((Control)(object)Druk, "Друк вибраного чека ");
		((ButtonBase)Druk).UseVisualStyleBackColor = true;
		((Control)TB2).Enabled = false;
		((Control)TB2).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TB2).Location = new Point(222, 29);
		((Control)TB2).Name = "TB2";
		((Control)TB2).Size = new Size(266, 30);
		((Control)TB2).TabIndex = 7;
		TB2.TextAlign = (HorizontalAlignment)2;
		PrintDialog1.UseEXDialog = true;
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(1337, 611);
		((Control)this).Controls.Add((Control)(object)GB1);
		((Control)this).Controls.Add((Control)(object)GB);
		((Control)this).Controls.Add((Control)(object)RetSheft);
		((Control)this).Controls.Add((Control)(object)DG1);
		((Control)this).Controls.Add((Control)(object)Tb);
		((Control)this).Controls.Add((Control)(object)DG);
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormReports";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Просмотр чеков";
		((ISupportInitialize)DG).EndInit();
		((ISupportInitialize)QrCode).EndInit();
		((ISupportInitialize)DG1).EndInit();
		((Control)GB).ResumeLayout(false);
		((Control)GB).PerformLayout();
		((Control)GB1).ResumeLayout(false);
		((Control)GB1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void LoadImg()
	{
		try
		{
			string text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".bmp";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".jpg";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\Logo\\" + All.A.FN + ".png";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
				return;
			}
			text = All.MyDoc() + "\\WebCheck\\logo.png";
			if (File.Exists(text))
			{
				PrintLogo = Image.FromFile(text);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private void FormReports_Load(object sender, EventArgs e)
	{
		Application.DoEvents();
		((Control)DG).Visible = true;
		((Control)DG1).Visible = false;
		((Control)GB).Visible = true;
		((Control)GB1).Visible = false;
		VidShift = true;
		((Control)RetSheft).Enabled = false;
		((Control)DG1).Left = ((Control)DG).Left;
		((Control)DG1).Top = ((Control)DG).Top;
		((Control)GB1).Left = ((Control)GB).Left;
		((Control)GB1).Top = ((Control)GB).Top;
		((Control)VibB).Enabled = false;
		if (Operators.CompareString(All.A.FiscalMode, All.URLfact, false) == 0)
		{
			GB.Text = "ФН " + All.A.FN;
		}
		else
		{
			GB.Text = "ФН " + All.A.FN + "_TS";
		}
		T1.Text = All.A.INN;
		T2.Text = All.A.TIN;
		T3.Text = All.A.PointName;
		T4.Text = All.A.PointAddr;
		LoadImg();
		int year = DateTime.Now.Year;
		int num = year;
		for (int i = 2019; i <= num; i = checked(i + 1))
		{
			Ycb.Items.Add((object)i.ToString());
		}
		Ycb.Text = Conversions.ToString(year);
		Mcb.Items.Add((object)"Січень");
		Mcb.Items.Add((object)"Лютий");
		Mcb.Items.Add((object)"Березень");
		Mcb.Items.Add((object)"Квітень");
		Mcb.Items.Add((object)"Травень");
		Mcb.Items.Add((object)"Червень");
		Mcb.Items.Add((object)"Липень");
		Mcb.Items.Add((object)"Серпень");
		Mcb.Items.Add((object)"Вересень");
		Mcb.Items.Add((object)"Жовтень");
		Mcb.Items.Add((object)"Листопад");
		Mcb.Items.Add((object)"Грудень");
		Mcb.Text = IntToMoon(DateTime.Now.Month.ToString());
		QrCode.Text = "http://www.webchek.com.ua";
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
		Application.DoEvents();
	}

	private string MoonToInt(string Ms)
	{
		return Ms switch
		{
			"Січень" => "01", 
			"Лютий" => "02", 
			"Березень" => "03", 
			"Квітень" => "04", 
			"Травень" => "05", 
			"Червень" => "06", 
			"Липень" => "07", 
			"Серпень" => "08", 
			"Вересень" => "09", 
			"Жовтень" => "10", 
			"Листопад" => "11", 
			"Грудень" => "12", 
			_ => "", 
		};
	}

	private string IntToMoon(string Mi)
	{
		return Mi switch
		{
			"1" => "Січень", 
			"2" => "Лютий", 
			"3" => "Березень", 
			"4" => "Квітень", 
			"5" => "Травень", 
			"6" => "Червень", 
			"7" => "Липень", 
			"8" => "Серпень", 
			"9" => "Вересень", 
			"10" => "Жовтень", 
			"11" => "Листопад", 
			"12" => "Грудень", 
			_ => "", 
		};
	}

	private bool ShiftsVisible(int nCheck)
	{
		ShiftsVisible();
		AllDoc(nCheck);
		return VidShift;
	}

	private bool ShiftsVisible()
	{
		((Control)VibB).Enabled = false;
		((Control)GB1).Enabled = false;
		if (((Control)DG).Visible)
		{
			((Control)DG).Visible = false;
			((Control)DG1).Visible = true;
			((Control)GB).Visible = false;
			((Control)GB1).Visible = true;
			VidShift = false;
			((Control)RetSheft).Enabled = true;
			return VidShift;
		}
		((Control)DG).Visible = true;
		((Control)DG1).Visible = false;
		((Control)GB).Visible = true;
		((Control)GB1).Visible = false;
		VidShift = true;
		((Control)RetSheft).Enabled = false;
		DG1.RowCount = 0;
		QrCode.Text = "http://www.webchek.com.ua";
		Tb.Text = "";
		TB2.Text = "";
		return VidShift;
	}

	private void Shefts()
	{
		checked
		{
			try
			{
				ShiftsAll shiftsAll = new ShiftsAll();
				DG.RowCount = 0;
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(0, i);
					DG[1, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(1, i);
					DG[2, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(2, i);
					DG[3, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(3, i);
					DG[4, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(4, i);
				}
				((Form)this).Text = "Зміни";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void Shefts(string Ms, string Ys)
	{
		checked
		{
			try
			{
				ShiftsAll shiftsAll = new ShiftsAll(Ms.Trim(), Ys.Trim());
				DG.RowCount = 0;
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					DataGridView dG;
					(dG = DG).RowCount = dG.RowCount + 1;
					DG[0, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(0, i);
					DG[1, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(1, i);
					DG[2, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(2, i);
					DG[3, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(3, i);
					DG[4, DG.RowCount - 1].Value = shiftsAll.get_InfaSheft(4, i);
				}
				((Form)this).Text = "Зміни";
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void CheckN(int numCheck)
	{
		checked
		{
			try
			{
				CheckShiftAll checkShiftAll = new CheckShiftAll(numCheck);
				int checks = checkShiftAll.Checks;
				for (int i = 1; i <= checks; i++)
				{
					DataGridView dG;
					(dG = DG1).RowCount = dG.RowCount + 1;
					DG1[0, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(0, i);
					DG1[1, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(1, i);
					DG1[2, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(2, i);
					DG1[3, DG1.RowCount - 1].Value = checkShiftAll.get_InfaCheck(3, i);
					DG1[4, DG1.RowCount - 1].Value = All.Bablo(checkShiftAll.get_InfaCheck(4, i));
				}
				((Form)this).Text = "Зміна №" + numCheck;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void AllDoc(int numShifts)
	{
		checked
		{
			try
			{
				AllDokShefts allDokShefts = new AllDokShefts(numShifts);
				int checks = allDokShefts.Checks;
				for (int i = 1; i <= checks; i++)
				{
					DataGridView dG;
					(dG = DG1).RowCount = dG.RowCount + 1;
					DG1[0, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(0, i);
					DG1[1, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(1, i);
					DG1[2, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(2, i);
					DG1[3, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(3, i);
					DG1[4, DG1.RowCount - 1].Value = All.Bablo(allDokShefts.get_InfaCheck(4, i));
					DG1[5, DG1.RowCount - 1].Value = allDokShefts.get_InfaCheck(5, i);
				}
				((Form)this).Text = "Зміна №" + numShifts;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				ProjectData.ClearProjectError();
			}
		}
	}

	private void RetSheft_Click(object sender, EventArgs e)
	{
		ShiftsVisible();
	}

	private void DG_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			string text = DG[0, ((DataGridViewBand)DG.CurrentRow).Index].Value.ToString();
			ShiftsVisible(Conversions.ToInteger(text));
		}
	}

	private void DG1_CellDoubleClick(object sender, DataGridViewCellEventArgs e)
	{
		if (e.RowIndex >= 0)
		{
			string numCheck = DG1[5, ((DataGridViewBand)DG1.CurrentRow).Index].Value.ToString();
			GB1.Text = "Чек № " + DG1[0, ((DataGridViewBand)DG1.CurrentRow).Index].Value.ToString();
			CheckZapoln(numCheck);
			if ((Operators.CompareString(DG1[2, ((DataGridViewBand)DG1.CurrentRow).Index].Value.ToString(), "продаж", false) == 0) | (Operators.CompareString(DG1[2, ((DataGridViewBand)DG1.CurrentRow).Index].Value.ToString(), "повернення", false) == 0))
			{
				((Control)VibB).Enabled = true;
			}
			else
			{
				((Control)VibB).Enabled = false;
			}
			((Control)GB1).Enabled = true;
		}
	}

	private bool CheckZapoln(string NumCheck)
	{
		PrintExportCheck printExportCheck = new PrintExportCheck();
		switch (All.f.IntegerGetFn(All.A.FN, "PrinterWidth"))
		{
		case 57:
			printExportCheck.Dlstr = 29;
			break;
		case 80:
			printExportCheck.Dlstr = 40;
			break;
		default:
			printExportCheck.Dlstr = 29;
			break;
		}
		QrCode.Text = printExportCheck.CheckVis(NumCheck);
		Tb.Text = printExportCheck.Tb;
		TB2.Text = printExportCheck.Tb2;
		LocalNumerCheck = Conversions.ToInteger(NumCheck);
		return true;
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

	private void Druk_Click(object sender, EventArgs e)
	{
		new PrintExportCheck().PrintCheck(Conversions.ToString(LocalNumerCheck), ((PictureBox)QrCode).Image, PrintLogo);
	}

	private void Button1_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}

	private void Zakrit_Click(object sender, EventArgs e)
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		//IL_000c: Invalid comparison between Unknown and I4
		if ((int)((CommonDialog)PrintDialog1).ShowDialog() == 1)
		{
			All.A.PrinterName = PrintDialog1.PrinterSettings.PrinterName;
			All.f.StringWriteFN(All.A.FN, "PrinterName", All.A.PrinterName);
			new PrintExportCheck().PrintCheck(Conversions.ToString(LocalNumerCheck), ((PictureBox)QrCode).Image, PrintLogo);
		}
	}

	private void Mcb_SelectedIndexChanged(object sender, EventArgs e)
	{
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
	}

	private void Ycb_SelectedIndexChanged(object sender, EventArgs e)
	{
		Shefts(MoonToInt(Mcb.Text), Ycb.Text);
	}

	private void Acb_CheckedChanged(object sender, EventArgs e)
	{
		if (Acb.Checked)
		{
			((Control)Ycb).Enabled = false;
			((Control)Mcb).Enabled = false;
			Shefts();
		}
		else
		{
			((Control)Ycb).Enabled = true;
			((Control)Mcb).Enabled = true;
			Shefts(MoonToInt(Mcb.Text), Ycb.Text);
		}
	}

	private void VibB_Click(object sender, EventArgs e)
	{
		//IL_0033: Unknown result type (might be due to invalid IL or missing references)
		if (Operators.CompareString(TB2.Text.Trim(), "", false) != 0)
		{
			FormViber formViber = new FormViber(TB2.Text.Trim());
			((Form)formViber).ShowDialog();
			((Component)(object)formViber).Dispose();
		}
	}

	private void QrCode_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(QrCode.Text);
	}
}
